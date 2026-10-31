import { useState } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { BUTTON_PRIMARY, CONTROL } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { BangMark } from '@/components/marks'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { AcquisitionPill } from '@/components/OfferedPills'
import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import {
  requiresDecisionNote,
  reviewerActions,
  type ReviewerAction,
} from '@/features/ops/transitions'
import { useOpsRequest, useReviewAction, useVillageEnergy } from '@/features/ops/useOpsRequests'
import { formatKw, formatKwh, formatTimestamp } from '@/lib/format'
import { useActorName } from '@/lib/actorNames'

const route = getRouteApi('/_ops/ops/requests/$requestId')

/**
 * Spec 7.3 — the review screen.
 *
 * Offers only legal transitions and surfaces the trigger's message verbatim if
 * one is raised anyway. The status machine itself lives in pue_request_guard;
 * this decides which controls exist, and an "illegal transition" error
 * reaching a user means this screen offered something it should not have.
 */
export function OpsRequestReviewScreen() {
  const { requestId } = route.useParams()
  const { t } = useTranslation()
  const query = useOpsRequest(requestId)
  // A name, never the raw id: resolved through the scoped provenance lookup.
  const capturedBy = useActorName(query.request?.captured_by)
  const energy = useVillageEnergy(query.request?.village_id)
  const review = useReviewAction(requestId, query.request?.village_id)

  const [note, setNote] = useState('')
  const [noteError, setNoteError] = useState(false)

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return (
      <Loading testId="ops-review-loading" />
    )
  }

  // Zero rows is an answer: RLS says this request is not visible.
  if (!query.request) {
    return <EmptyState title={t('ops.notFoundTitle')} detail={t('ops.notFoundDetail')} />
  }

  const request = query.request
  const estimate = request.estimate
  const actions = reviewerActions(request.status)

  const run = (action: ReviewerAction) => {
    if (requiresDecisionNote(action) && note.trim() === '') {
      setNoteError(true)
      return
    }
    setNoteError(false)
    review.mutate({ action, note: requiresDecisionNote(action) ? note.trim() : undefined })
  }

  return (
    <section className="flex w-full flex-col gap-[18px]" data-testid="request-review">
      <PageHeader
        title={<>{request.equipment_name} <StatusPill kind="request" status={request.status} /> <AcquisitionPill mode={request.acquisition} /></>}
        backTo="/ops/requests"
        backLabel={t('tour.back')}
        breadcrumbs={[{ label: t('nav.requests'), to: '/ops/requests' }, { label: request.equipment_name }]}
      />
      <header className="flex flex-col gap-2.5">
        <ProvenanceBadge source={request.source} verification={request.verification} confidence={request.confidence} capturedAt={request.captured_at} capturedBy={capturedBy} />
        <dl className="grid gap-x-5 gap-y-1.5 sm:grid-cols-2">
          <Row label={t('ops.applicant')} value={request.applicant} />
          <Row label={t('ops.village')} value={request.village_name} />
          <Row label={t('ops.farm')} value={request.farm_label ?? '—'} />
          {request.submitted_at && (
            <Row label={t('ops.colSubmitted')} value={formatTimestamp(request.submitted_at)} />
          )}
        </dl>
        {request.purpose && (
          <p style={{ fontSize: 15, lineHeight: 1.55, color: 'var(--ink-2)' }}>{request.purpose}</p>
        )}
      </header>

      {/*
        The inputs the estimate was calculated from, as they were at the moment
        it was stored. A snapshot of an estimate, so neither half is a
        measurement; the wording below says so.
      */}
      <section
        className="flex flex-col gap-2 p-4"
        data-testid="review-estimate"
        style={{
          border: '1px solid var(--rule-2)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--paper)',
        }}
      >
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('ops.snapshotted')}
        </h2>
        <p className="type-note" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('ops.snapshottedNote')}
        </p>
        {estimate ? (
          <dl className="grid gap-x-5 gap-y-1.5 sm:grid-cols-2">
            <Row label={t('estimate.ratedPower')} value={formatKw(estimate.rated_power_kw)} />
            <Row label={t('estimate.quantity')} value={String(estimate.quantity)} />
            <Row label={t('estimate.hours')} value={String(estimate.hours_per_day)} />
            <Row label={t('estimate.days')} value={String(estimate.days_per_week)} />
            <Row label={t('estimate.peak')} value={formatKw(estimate.est_power_kw)} />
            <Row label={t('estimate.perWeek')} value={formatKwh(estimate.est_kwh_per_week)} />
          </dl>
        ) : (
          <EmptyState title={t('requests.noEstimate')} detail={t('requests.noEstimateDetail')} />
        )}
        <p style={{ fontSize: 13, lineHeight: 1.5, fontWeight: 500 }}>{t('estimate.isEstimate')}</p>
      </section>

      <section className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('ops.headroom')}
        </h2>
        {energy.isLoading ? (
          <Loading />
        ) : energy.data ? (
          <>
            <div className="flex flex-wrap gap-2.5">
              <Cell
                label={t('ops.capacity')}
                value={formatKw(energy.data.capacity_kw)}
                /* Capacity is PLANNED, never measured. The basis is in the
                   cell, not in a row of its own. */
                pill={t(`capacityBasis.${energy.data.capacity_basis}`)}
              />
              <Cell
                label={t('ops.headroom')}
                value={formatKw(energy.data.headroom_kw)}
                testId="review-headroom"
              />
            </div>

            {/* Two cells with a rule between them: no layout here sums them. */}
            <div
              data-testid="review-peaks"
              className="flex flex-wrap items-stretch overflow-hidden"
              style={{ border: '1px solid var(--rule-2)', borderRadius: 'var(--radius-card)' }}
            >
              <Peak
                label={t('ops.prospectivePeak')}
                value={formatKw(energy.data.prospective_peak_kw)}
              />
              <span
                aria-hidden
                style={{ flex: '0 0 auto', width: '100%', height: 1, background: 'var(--rule-2)' }}
              />
              <Peak
                label={t('ops.approvedPeak')}
                value={formatKw(energy.data.approved_peak_kw)}
                testId="review-approved-peak"
                decided
              />
            </div>

            <dl className="grid gap-x-5 gap-y-1.5 sm:grid-cols-2">
              <Row
                label={t('ops.simultaneity')}
                value={String(energy.data.simultaneity_factor)}
              />
            </dl>
          </>
        ) : (
          <EmptyState title={t('ops.noHeadroom')} detail={t('ops.noHeadroomDetail')} />
        )}
        {/* Prospective and approved are separate figures, never summed. */}
        <p style={{ fontSize: 13, lineHeight: 1.5, fontWeight: 500, textWrap: 'pretty' }}>
          {t('ops.neverSummed')}
        </p>
      </section>

      {request.decision_note && (
        <section className="flex flex-col gap-1.5">
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

      {review.isError && (
        <div data-testid="review-error">
          <ErrorState error={review.error} onRetry={() => review.reset()} />
        </div>
      )}

      {actions.length === 0 ? (
        <p style={{ fontSize: 15, color: 'var(--ink-2)' }}>{t('ops.noActions')}</p>
      ) : (
        <section data-testid="review-decision" className="flex flex-col gap-3">
          {actions.some(requiresDecisionNote) && (
            <div className="flex flex-col gap-1.5">
              <label
                className="block"
                htmlFor="decision-note"
                style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}
              >
                {t('ops.decisionNote')}
              </label>
              <textarea
                id="decision-note"
                data-testid="decision-note"
                rows={2}
                value={note}
                onChange={(e) => setNote(e.target.value)}
                aria-invalid={noteError || undefined}
                aria-describedby={noteError ? 'decision-note-error' : undefined}
                className="w-full"
                style={
                  noteError ? { ...CONTROL, border: '1.5px solid var(--flag-ink)' } : CONTROL
                }
              />
              {noteError && (
                <p
                  id="decision-note-error"
                  data-testid="decision-note-error"
                  role="alert"
                  className="flex items-start gap-[7px] font-medium"
                  style={{ fontSize: 13, color: 'var(--flag-ink)' }}
                >
                  <BangMark />
                  {t('ops.decisionNoteRequired')}
                </p>
              )}
            </div>
          )}

          <div className="flex flex-wrap gap-2">
            {actions.map((action) => (
              <button
                key={action}
                type="button"
                data-testid={`action-${action}`}
                disabled={review.isPending}
                onClick={() => run(action)}
                className="disabled:opacity-60"
                style={BUTTON_PRIMARY}
              >
                {review.isPending ? t('ops.working') : t(`ops.${camel(action)}`)}
              </button>
            ))}
          </div>
        </section>
      )}
    </section>
  )
}

function camel(action: ReviewerAction): string {
  return action === 'start_review' ? 'startReview' : action
}

function Row({ label, value, testId }: { label: string; value: string; testId?: string }) {
  return (
    <div
      className="grid items-baseline gap-3"
      style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}
    >
      <dt style={{ fontSize: 13, color: 'var(--ink-2)' }}>{label}</dt>
      <dd data-testid={testId} className="tabular font-semibold" style={{ fontSize: 15 }}>
        {value}
      </dd>
    </div>
  )
}

/** A figure in a cell of its own, with anything that qualifies it inside. */
function Cell({
  label,
  value,
  testId,
  pill,
}: {
  label: string
  value: string
  testId?: string
  pill?: string
}) {
  return (
    <div
      className="flex min-w-0 flex-col gap-1 px-3.5 py-3"
      style={{
        flex: '1 1 180px',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-control)',
        background: 'var(--sand-2)',
      }}
    >
      <span className="type-note" style={{ color: 'var(--ink-2)' }}>
        {label}
      </span>
      <span className="flex flex-wrap items-baseline gap-2">
        <span data-testid={testId} className="tabular type-figure">
          {value}
        </span>
        {pill && (
          <span
            data-testid="review-capacity-basis"
            className="type-column-label px-2 py-[3px]"
            style={{
              border: '1px solid var(--rule-2)',
              borderRadius: 'var(--radius-pill)',
              background: 'var(--paper)',
              color: 'var(--ink-2)',
              fontWeight: 700,
            }}
          >
            {pill}
          </span>
        )}
      </span>
    </div>
  )
}

/** One of the two peaks the review must never let be read as a sum. */
function Peak({
  label,
  value,
  testId,
  decided = false,
}: {
  label: string
  value: string
  testId?: string
  decided?: boolean
}) {
  return (
    <div
      data-peak-cell={decided ? 'approved' : 'prospective'}
      className="flex flex-col gap-0.5 p-3.5"
      style={{
        flex: '1 1 160px',
        minWidth: 0,
        background: decided ? 'var(--primary-tint)' : 'var(--paper)',
      }}
    >
      <span className="type-note" style={{ color: decided ? 'var(--primary-ink)' : 'var(--ink-3)' }}>
        {label}
      </span>
      <span
        data-testid={testId}
        className="tabular type-figure"
        style={decided ? { color: 'var(--primary-ink)' } : undefined}
      >
        {value}
      </span>
    </div>
  )
}
