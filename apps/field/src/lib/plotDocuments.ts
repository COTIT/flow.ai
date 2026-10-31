import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'

/**
 * Plot title documents — the one exception to "no photo upload", approved on
 * 5 Oct 2026 (migration 20261005090003_plot_documents).
 *
 * Staff only: the bucket is private and its policies, like `plot_document`'s,
 * admit field officers, ops and admin of the plot's village. Nothing here
 * checks that; RLS does. Append-only: there is no update or delete.
 *
 * Size and type are the bucket's rules (10 MB; images and PDFs). The client
 * does not copy them: a refused file comes back with storage's own message.
 */
export const PLOT_DOCUMENT_BUCKET = 'plot-documents'

/** What the file picker offers. The bucket still decides what it accepts. */
export const PLOT_DOCUMENT_ACCEPT = 'image/*,application/pdf'

/** A picked file, as kept in the register draft until the plot exists. */
export interface DraftFile {
  /** Chosen at pick time, so a retried upload lands on the same path. */
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
 * `{village}/{plot}/{id}-{name}`. The storage policy reads the village from
 * the first folder; the row's check reads the plot from the second.
 */
export function plotDocumentPath(villageId: string, plotId: string, id: string, fileName: string): string {
  const safe = fileName
    .normalize('NFKD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/[^A-Za-z0-9.]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(-100)
  return `${villageId}/${plotId}/${id}-${safe || 'file'}`
}

/** Storage's "already there" — a retried upload, not a failure. */
const alreadyStored = (error: { message?: string; statusCode?: string | number }) =>
  String(error.statusCode) === '409' || /already exists/i.test(error.message ?? '')

/**
 * Store each file, then file the row describing it. One refused file does not
 * stop the others. A file already stored and filed (a retry) counts as filed.
 */
export async function uploadPlotDocuments(
  villageId: string,
  plotId: string,
  files: DraftFile[],
): Promise<UploadResult> {
  const result: UploadResult = { filed: [], failed: [] }

  for (const file of files) {
    const path = plotDocumentPath(villageId, plotId, file.id, file.name)
    const stored = await supabase.storage
      .from(PLOT_DOCUMENT_BUCKET)
      .upload(path, file.blob, { contentType: file.type, upsert: false })
    if (stored.error && !alreadyStored(stored.error)) {
      result.failed.push({ id: file.id, name: file.name, message: stored.error.message })
      continue
    }

    const { error } = await supabase.from('plot_document').insert({
      plot_id: plotId,
      village_id: villageId,
      storage_path: path,
      file_name: file.name,
      mime_type: file.type,
      size_bytes: file.size,
    })
    if (error && error.code !== '23505') {
      result.failed.push({ id: file.id, name: file.name, message: error.message })
      continue
    }
    result.filed.push(file.id)
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
