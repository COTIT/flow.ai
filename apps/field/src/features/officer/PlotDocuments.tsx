import { useTranslation } from 'react-i18next'
import { FileImage, FileText, Paperclip } from 'lucide-react'

import { ErrorState } from '@/components/ErrorState'
import { newUuid } from '@/lib/ids'
import {
  PLOT_DOCUMENT_ACCEPT,
  usePlotDocuments,
  useUploadPlotDocuments,
} from '@/lib/plotDocuments'

/**
 * The title documents filed against one plot — where the verifier checks the
 * plot's fields against what the officer photographed.
 *
 * Staff only, append-only. RLS on `plot_document` and the bucket decides who
 * sees and adds; this renders whatever comes back, and zero rows is an answer.
 */
export function PlotDocuments({ plotId, villageId }: { plotId: string; villageId: string }) {
  const { t } = useTranslation()
  const query = usePlotDocuments(plotId)
  const upload = useUploadPlotDocuments(villageId, plotId)
  const docs = query.data ?? []
  const inputId = `plot-documents-add-${plotId}`

  return (
    <section
      data-testid={`plot-documents-${plotId}`}
      className="flex flex-col gap-2 px-3 py-2.5"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h4 style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
          {t('plotDocuments.title', { count: docs.length })}
        </h4>
        <span className="type-note" style={{ color: 'var(--ink-3)' }}>{t('plotDocuments.staffOnly')}</span>
      </div>

      {query.error ? (
        <ErrorState error={query.error} onRetry={() => void query.refetch()} />
      ) : query.isLoading ? (
        <p className="type-note" style={{ color: 'var(--ink-3)' }}>{t('common.loading')}</p>
      ) : docs.length === 0 ? (
        <p data-testid="plot-documents-empty" className="type-note" style={{ color: 'var(--ink-3)' }}>
          {t('plotDocuments.none')}
        </p>
      ) : (
        <ul className="flex flex-col gap-1">
          {docs.map((doc) => (
            <li key={doc.id}>
              <a
                data-testid="plot-document"
                href={doc.url ?? undefined}
                target="_blank"
                rel="noreferrer noopener"
                className="inline-flex max-w-full items-center gap-2 font-medium underline-offset-2 hover:underline"
                style={{ fontSize: 14, color: 'var(--primary-ink)' }}
              >
                {doc.mime_type === 'application/pdf' ? <FileText size={16} aria-hidden /> : <FileImage size={16} aria-hidden />}
                <span className="truncate">{doc.file_name}</span>
              </a>
            </li>
          ))}
        </ul>
      )}

      {upload.data && upload.data.failed.length > 0 && (
        <ul data-testid="plot-documents-failed" className="type-note flex flex-col gap-1" style={{ color: 'var(--flag-ink)' }}>
          {upload.data.failed.map((f) => (
            <li key={f.id}>{f.name}: {f.message}</li>
          ))}
        </ul>
      )}
      {upload.isError && <ErrorState error={upload.error} />}

      <label
        htmlFor={inputId}
        className="inline-flex w-fit cursor-pointer items-center gap-2 font-medium"
        style={{ fontSize: 14, color: 'var(--primary-ink)', minHeight: 40 }}
      >
        <Paperclip size={16} aria-hidden />
        {upload.isPending ? t('plotDocuments.adding') : t('plotDocuments.add')}
      </label>
      <input
        id={inputId}
        data-testid={inputId}
        type="file"
        multiple
        accept={PLOT_DOCUMENT_ACCEPT}
        className="sr-only"
        disabled={upload.isPending}
        onChange={(e) => {
          const picked = Array.from(e.target.files ?? [])
          e.target.value = ''
          if (picked.length === 0) return
          upload.mutate(picked.map((file) => ({ id: newUuid(), name: file.name, type: file.type, size: file.size, blob: file })))
        }}
      />
    </section>
  )
}
