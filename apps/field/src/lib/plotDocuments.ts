import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { farmDocumentsCreate } from '@genesy/api-client/endpoints/farm-documents'

import { workspaceId } from '@/app/workspace'
import { apiErrorMessage } from '@/lib/apiError'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'

/**
 * Plot title documents — the one exception to "no photo upload", approved on
 * 5 Oct 2026.
 *
 * Staff only: the API admits field officers, ops and admin of the plot's
 * village. Nothing here checks that. Append-only: there is no update or delete.
 *
 * Size and type are the API's rules (10 MB; images and PDFs, read from the
 * bytes). The client does not copy them: a refused file comes back with the
 * API's own message.
 *
 * shortcut: the list below (person detail) still reads Supabase storage; it moves with that screen.
 */
export const PLOT_DOCUMENT_BUCKET = 'plot-documents'

/** What the file picker offers. The bucket still decides what it accepts. */
export const PLOT_DOCUMENT_ACCEPT = 'image/*,application/pdf'

/** A picked file, as kept in the register draft until the plot exists. */
export interface DraftFile {
  /** Chosen at pick time, so a retried upload lands on the same document. */
  id: string
  name: string
  type: string
  size: number
  blob: Blob
}

export interface PlotDocument {
  id: string
  file_name: string
  mime_type: string
  size_bytes: number
  uploaded_at: string
  storage_path: string
  /** A short-lived signed link. Absent when signing failed. */
  url: string | null
}

export interface UploadResult {
  filed: string[]
  failed: Array<{ id: string; name: string; message: string }>
}

/**
 * File each picked document against the plot: `POST …/farm/plots/{plot}/documents`, one multipart
 * call per file, which stores it and files its row together. One refused file does not stop the
 * others. The device id makes a retry safe: the API answers 200 with the document already filed.
 *
 * `_villageId`: kept so callers keep their shape; the API reads the village from the plot.
 */
export async function uploadPlotDocuments(
  _villageId: string,
  plotId: string,
  files: DraftFile[],
): Promise<UploadResult> {
  const result: UploadResult = { filed: [], failed: [] }

  for (const file of files) {
    try {
      // A File, so the multipart part carries the name the officer picked.
      const named = new File([file.blob], file.name, { type: file.type })
      await farmDocumentsCreate(workspaceId(), plotId, { file: named, id: file.id })
      result.filed.push(file.id)
    } catch (error) {
      const message = apiErrorMessage(error) ?? (error instanceof Error ? error.message : String(error))
      result.failed.push({ id: file.id, name: file.name, message })
    }
  }

  return result
}

/** Shape guard for the stored list of picked files (QA #22's lesson). */
export function isDraftFiles(value: unknown): value is DraftFile[] {
  return (
    Array.isArray(value) &&
    value.every(
      (f) =>
        typeof f === 'object' &&
        f !== null &&
        typeof f.id === 'string' &&
        typeof f.name === 'string' &&
        typeof f.type === 'string' &&
        typeof f.size === 'number' &&
        f.blob instanceof Blob,
    )
  )
}

/** Signed links last ten minutes: long enough to look, short enough not to share. */
const SIGNED_URL_SECONDS = 600

export async function fetchPlotDocuments(plotId: string): Promise<PlotDocument[]> {
  const { data, error } = await supabase
    .from('plot_document')
    .select('id, file_name, mime_type, size_bytes, uploaded_at, storage_path')
    .eq('plot_id', plotId)
    .order('uploaded_at')
  if (error) throw new Error(error.message)
  const rows = data ?? []
  if (rows.length === 0) return []

  const signed = await supabase.storage
    .from(PLOT_DOCUMENT_BUCKET)
    .createSignedUrls(rows.map((r) => r.storage_path), SIGNED_URL_SECONDS)
  const urls = new Map((signed.data ?? []).map((s) => [s.path, s.signedUrl]))

  return rows.map((r) => ({ ...r, url: urls.get(r.storage_path) ?? null }))
}

/** Zero rows is an answer: no documents, or not yours to see. */
export function usePlotDocuments(plotId: string) {
  return useQuery({
    queryKey: queryKeys.plotDocuments(plotId),
    queryFn: () => fetchPlotDocuments(plotId),
    // Refetch before the signed links expire.
    staleTime: (SIGNED_URL_SECONDS - 60) * 1000,
  })
}

export function useUploadPlotDocuments(villageId: string, plotId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (files: DraftFile[]) => uploadPlotDocuments(villageId, plotId, files),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.plotDocuments(plotId) }),
  })
}
