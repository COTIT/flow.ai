import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { BUTTON_PRIMARY, BUTTON_SECONDARY } from '@/components/controlStyles'
import { Card, Loading, ProductNote } from '@/components/controls'
import { StatusPill } from '@/components/StatusPill'
import { AcquisitionPill } from '@/components/OfferedPills'
import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import { useFarmerTransition, useRequest } from '@/features/farmer/useRequests'
import { farmerActions } from '@/features/ops/transitions'
import { formatKw, formatKwh, formatTimestamp } from '@/lib/format'
import { useActorName } from '@/lib/actorNames'

const route = getRouteApi('/_farmer/farm/requests/$requestId')

/**
 * Spec 6.5 — the equipment, the assumptions, the STORED estimate, and the
 * decision note once decided.
 *
 * The estimate here is read from energy_estimate, not recomputed: the trigger
 * is its only writer, and showing the stored row is what lets the preview and
 * the stored figure be compared rather than assumed identical.
 *
 * A submitted request is not editable. pue_request_guard freezes the content
 * once it leaves draft, so no edit control is offered — rendering one would be
 * a lie about what is possible.
 */
export function RequestDetailScreen() {
  const { requestId } = route.useParams()
  const { t } = useTranslation()
  const query = useRequest(requestId)
  // A name, never the raw id: resolved through the scoped provenance lookup.
  const capturedBy = useActorName(query.request?.captured_by)
  const transition = useFarmerTransition(requestId, query.request?.village_id ?? undefined)

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return <Loading testId="request-detail-loading" />
  }

  // Zero rows is an answer: RLS says this request is not visible.
  if (!query.request) {
    return <EmptyState title={t('requests.notFoundTitle')} detail={t('requests.notFoundDetail')} />
  }

  const request = query.request
  const estimate = request.estimate
  const frozen = request.status !== 'draft'
  const actions = farmerActions(request.status)

  return (
    <section className="flex max-w-lg flex-col gap-4" data-testid="request-detail">
      <header className="flex flex-col gap-2.5">
        <div className="flex flex-wrap items-center gap-2.5">
          <h1 className="type-screen-title">{request.equipment_name}</h1>
          <StatusPill kind="request" status={request.status} />
          <AcquisitionPill mode={request.acquisition} />
        </div>
        <ProvenanceBadge source={request.source} verification={request.verification} confidence={request.confidence} capturedAt={request.captured_at} capturedBy={capturedBy} />
        {request.purpose && (
          <p style={{ fontSize: 15, color: 'var(--ink-2)' }}>{request.purpose}</p>
        )}
        {request.submitted_at && (
          <p className="type-note" style={{ color: 'var(--ink-3)' }}>
            {t('requests.submittedOn')} {formatTimestamp(request.submitted_at)}
          </p>
        )}
      </header>

      <section data-testid="request-assumptions" className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('requests.assumptions')}
        </h2>
        <Card className="flex flex-wrap gap-4 p-4" sunken>
          <Assumption label={t('equipment.quantity')} value={String(request.quantity)} />
          <Assumption label={t('equipment.hours')} value={String(request.hours_per_day ?? '—')} />
          <Assumption label={t('equipment.days')} value={String(request.days_per_week ?? '—')} />
        </Card>
      </section>

      <section
        data-testid="request-stored-estimate"
        className="flex flex-col gap-2.5 p-4"
        style={{
          border: '1px solid var(--rule-2)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--paper)',
        }}
      >
        <h2 className="type-section inline-flex flex-wrap items-center gap-2" style={{ color: 'var(--ink-3)' }}>
          {t('requests.storedEstimate')}
          <span
            className="type-microlabel px-2 py-px"
            style={{
              border: '1px solid var(--rule-2)',
              borderRadius: 'var(--radius-pill)',
              background: 'var(--paper)',
              color: 'var(--ink-2)',
            }}
          >
            {t('estimate.tag')}
          </span>
        </h2>
        {estimate ? (
          <dl className="flex flex-col gap-1.5">
            <Row
              label={t('estimate.peak')}
              value={formatKw(estimate.est_power_kw)}
              testId="stored-estimate-power"
            />
            <Row
              label={t('estimate.perDay')}
              value={formatKwh(estimate.est_kwh_per_day)}
              testId="stored-estimate-kwh-day"
            />
            <Row
              label={t('estimate.perWeek')}
              value={formatKwh(estimate.est_kwh_per_week)}
              testId="stored-estimate-kwh-week"
            />
          </dl>
        ) : (
          <EmptyState title={t('requests.noEstimate')} detail={t('requests.noEstimateDetail')} />
        )}
        <p style={{ fontSize: 13, lineHeight: 1.5, fontWeight: 500 }}>{t('estimate.isEstimate')}</p>
      </section>

      {request.decision_note && (
        <section data-testid="request-decision" className="flex flex-col gap-1.5">
          <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
            {t('requests.decision')}
          </h2>
          <p style={{ fontSize: 15, lineHeight: 1.55 }}>{request.decision_note}</p>
          {request.decided_at && (
            <p className="type-note" style={{ color: 'var(--ink-3)' }}>
              {formatTimestamp(request.decided_at)}
            </p>
          )}
        </section>
      )}

      {frozen && <ProductNote>{t('requests.frozen')}</ProductNote>}

      {/* What the applicant may do from here — business-rules §2's role
          matrix, mirrored by farmerActions. A draft can be submitted or
          withdrawn; a submitted request withdrawn; anything under review or
          decided offers nothing, because it is no longer theirs to move. */}
      {actions.length > 0 && (
        <section className="flex flex-col gap-2.5">
          {transition.error && (
            <p
              data-testid="request-action-error"
              role="alert"
              className="px-3.5 py-3"
              style={{
                border: '1px solid rgba(158, 27, 27, .25)',
                borderLeft: '4px solid var(--flag-ink)',
                borderRadius: 'var(--radius-card)',
                background: 'var(--flag-tint)',
                fontSize: 14,
                lineHeight: 1.55,
              }}
            >
              {transition.error.message}
            </p>
          )}

          <div className="flex flex-wrap gap-2.5">
            {actions.map((action) => (
              <button
                key={action}
                type="button"
                data-testid={`request-action-${action}`}
                disabled={transition.isPending}
                onClick={() => transition.mutate(action)}
                className="flex-1 disabled:opacity-60"
                style={
                  action === 'submit'
                    ? { ...BUTTON_PRIMARY, minHeight: 48, fontSize: 16 }
                    : { ...BUTTON_SECONDARY, minHeight: 48, fontSize: 16 }
                }
              >
                {t(`requests.action.${action}`)}
              </button>
            ))}
          </div>

          {request.status === 'draft' && (
            <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
              {t('requests.draftNote')}
            </p>
          )}
        </section>
      )}
    </section>
  )
}

/**
 * A label that may wrap to two lines with its figure still pinned right — a
 * grid track, not `justify-between` on a flex row.
 */
function Row({ label, value, testId }: { label: string; value: string; testId?: string }) {
  return (
    <div
      className="grid items-baseline gap-3"
      style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}
    >
      <dt style={{ fontSize: 13, color: 'var(--ink-2)' }}>{label}</dt>
      <dd data-testid={testId} className="tabular font-semibold" style={{ fontSize: 17 }}>
        {value}
      </dd>
    </div>
  )
}

/** One of the three figures the stored estimate was calculated from. */
function Assumption({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-px">
      <span className="type-note" style={{ color: 'var(--ink-3)' }}>
        {label}
      </span>
      <span className="tabular font-semibold" style={{ fontSize: 17 }}>
        {value}
      </span>
    </div>
  )
}
