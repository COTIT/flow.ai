import { useTranslation } from 'react-i18next'
import { Link } from '@tanstack/react-router'

import { useScopeNames } from '@/app/scope'
import { useSession } from '@/app/session'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import { VerificationMark } from '@/components/marks'
import { VerifyButton } from '@/features/officer/VerifyButton'
import { useVerifyFromQueue, useVerifyQueue, type QueueRow } from '@/features/officer/useVerifyQueue'
import { getVerifyTarget } from '@/features/officer/verifyNavigation'

/**
 * Spec 5.7 — the officer's verify queue: records still carrying
 * `unverified` or `pending` in their villages.
 *
 * Verification is a deliberate act with the verifier's name attached
 * (business-rules §5), never a checkbox inside an edit form — so each row is
 * its own decision, and verifying one leaves the rest alone.
 *
 * State reads down the left edge: the mark leads every row, so the queue can be
 * scanned for what is outstanding without reading a word of it.
 */
export function VerifyQueueScreen() {
  const { t } = useTranslation()
  const query = useVerifyQueue()
  const verify = useVerifyFromQueue()
  const scope = useScopeNames()
  const userId = useSession().data?.userId

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  const rows = query.data ?? []

  return (
    <section data-testid="verify-queue" className="flex w-full flex-col gap-4">
      <header className="flex flex-col gap-1.5">
        <div className="flex flex-wrap items-baseline gap-3">
          <h1 className="type-screen-title">{t('verifyQueue.title')}</h1>
          {rows.length > 0 && (
            <span
              data-testid="verify-queue-count"
              className="type-note px-2.5 py-1 font-semibold"
              style={{
                border: '1px solid var(--rule-2)',
                borderRadius: 'var(--radius-pill)',
                background: 'var(--sand-2)',
                color: 'var(--ink-2)',
              }}
            >
              {t('verifyQueue.outstanding', { count: rows.length })}
            </span>
          )}
        </div>
        <p style={{ fontSize: 13, lineHeight: 1.5, color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('verifyQueue.intro')}
        </p>
      </header>

      {/* app_verify's own message if it refuses — shown as written, never
          replaced with a generic failure (business-rules §9). */}
      {verify.error && <ErrorState error={verify.error} />}

      {query.isLoading ? (
        <p data-testid="verify-queue-loading" style={{ fontSize: 15, color: 'var(--ink-2)' }}>
          {t('common.loading')}
        </p>
      ) : rows.length === 0 ? (
        // Nothing outstanding is a result worth stating plainly, not a blank
        // list and never an error.
        <EmptyState title={t('verifyQueue.noneTitle')} detail={t('verifyQueue.noneDetail')} />
      ) : (
        <ul
          className="flex flex-col overflow-hidden"
          style={{
            border: '1px solid var(--rule)',
            borderRadius: 'var(--radius-card)',
            background: 'var(--paper)',
          }}
        >
          {rows.map((row, index) => (
            <li
              key={`${row.table}:${row.id}`}
              data-testid="verify-queue-row"
              data-table={row.table}
              className="flex flex-wrap items-center gap-3 px-4 py-3"
              style={{
                minHeight: 48,
                ...(index > 0 ? { borderTop: '1px solid var(--rule)' } : {}),
              }}
            >
              {(() => {
                const target = getVerifyTarget(row)
                return target ? (
                <Link
                  to={target.to as never}
                  params={target.params as never}
                  search={target.search as never}
                  data-testid="verify-record-link"
                  className="flex min-w-0 flex-1 items-center gap-3 text-left"
                  style={{ color: 'inherit', textDecoration: 'none' }}
                >
                  <QueueSummary
                    row={row}
                    tableLabel={t(`verifyQueue.table.${row.table}`)}
                    villageLabel={scope.data?.villages[row.village_id] ?? row.village_id}
                  />
                </Link>
                ) : (
                <div className="flex min-w-0 flex-1 items-center gap-3">
                  <QueueSummary
                    row={row}
                    tableLabel={t(`verifyQueue.table.${row.table}`)}
                    villageLabel={scope.data?.villages[row.village_id] ?? row.village_id}
                  />
                </div>
                )
              })()}

              {needsSecondStaff(row, userId) ? (
                <SecondStaffNote id={row.id} />
              ) : (
                <VerifyButton
                  table={row.table}
                  id={row.id}
                  recordLabel={row.label}
                  verification={row.verification}
                  pending={verify.isPending}
                  onVerify={(target) => verify.mutateAsync(target)}
                />
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}

/**
 * Four eyes on households (20260929090002_household_four_eyes): the officer
 * who registered one may not verify it, because a household is what a survey
 * incentive is paid to. app_verify refuses regardless; this only spares the
 * officer a button that can only fail. Other records keep the old rule.
 */
function needsSecondStaff(row: QueueRow, userId: string | undefined): boolean {
  return row.table === 'household' && userId !== undefined && row.captured_by === userId
}

function SecondStaffNote({ id }: { id: string }) {
  const { t } = useTranslation()
  return (
    <p
      data-testid={`verify-needs-second-staff-${id}`}
      className="type-note max-w-56"
      style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}
    >
      {t('verifyQueue.householdNeedsSecondStaff')}
    </p>
  )
}

function QueueSummary({
  row,
  tableLabel,
  villageLabel,
}: {
  row: QueueRow
  tableLabel: string
  villageLabel: string
}) {
  return (
    <>
      {/* The mark leads. State is legible before any word is read. */}
      <VerificationMark verification={row.verification} size={20} />

      <div className="flex min-w-0 flex-1 flex-col gap-1">
                <p className="flex flex-wrap items-center gap-2">
                  <span
                    className="type-note px-2 py-0.5"
                    style={{
                      border: '1px solid var(--rule-2)',
                      borderRadius: 'var(--radius-pill)',
                      background: 'var(--sand-2)',
                      color: 'var(--ink-2)',
                    }}
                  >
                    {tableLabel}
                  </span>
                  <b style={{ fontSize: 16, fontWeight: 600 }}>{row.label}</b>
                </p>
                <ProvenanceBadge
                  compact
                  recordLabel={villageLabel}
                  source={row.source}
                  verification={row.verification}
                  confidence={row.confidence ?? undefined}
                  capturedAt={row.captured_at}
                />
      </div>
    </>
  )
}
