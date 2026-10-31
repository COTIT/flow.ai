import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Loading } from '@/components/controls'
import { StatusPill } from '@/components/StatusPill'
import { AcquisitionPill } from '@/components/OfferedPills'
import { useMyRequests } from '@/features/farmer/useRequests'
import { formatTimestamp } from '@/lib/format'

/** Spec 6.5 — the farmer's own requests, each with a status pill. */
export function RequestsListScreen() {
  const { t } = useTranslation()
  const query = useMyRequests()

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return <Loading testId="requests-loading" />
  }

  if (query.requests.length === 0) {
    return <EmptyState title={t('requests.noneTitle')} detail={t('requests.noneDetail')} />
  }

  return (
    <section className="flex flex-col gap-3.5" data-testid="requests-list">
      <h1 className="type-screen-title">{t('requests.title')}</h1>

      <ul className="flex flex-col gap-2.5">
        {query.requests.map((request) => (
          <li key={request.id}>
            <Link
              to="/farm/requests/$requestId"
              params={{ requestId: request.id }}
              data-testid="my-request-row"
              data-status={request.status}
              className="flex flex-col gap-1.5 p-4 hover:bg-primary-tint"
              style={{
                minHeight: 52,
                border: '1px solid var(--rule)',
                borderRadius: 'var(--radius-card)',
                background: 'var(--paper)',
                color: 'var(--ink)',
              }}
            >
              <span className="flex flex-wrap items-center gap-2.5">
                <b style={{ fontSize: 16, fontWeight: 600 }}>{request.equipment_name}</b>
                <StatusPill kind="request" status={request.status} />
                <AcquisitionPill mode={request.acquisition} />
              </span>
              {request.purpose && (
                <span style={{ fontSize: 13, color: 'var(--ink-2)' }}>{request.purpose}</span>
              )}
              {request.submitted_at && (
                <span className="type-note" style={{ color: 'var(--ink-3)' }}>
                  {t('requests.submittedOn')} {formatTimestamp(request.submitted_at)}
                </span>
              )}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  )
}
