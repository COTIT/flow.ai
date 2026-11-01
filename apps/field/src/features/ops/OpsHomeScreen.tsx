import type { ReactNode } from 'react'
import { ArrowRight, CircleCheckBig, ClipboardList, Package } from 'lucide-react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { ErrorState } from '@/components/ErrorState'
import { Loading } from '@/components/controls'
import { PageHeader } from '@/components/PageHeader'
import { useOpsHome } from '@/features/ops/useOpsHome'
import { resourcesEnabled } from '@/lib/features'

type LinkTo = Parameters<typeof Link>[0]['to']

/**
 * One queue count, and the list it belongs to.
 *
 * Every figure is a link: a count an ops user cannot act on is a statistic,
 * and this screen exists to start work rather than to report on it.
 */
function Queue({
  label,
  icon,
  value,
  detail,
  to,
  search,
  testId,
  rule,
  action,
}: {
  label: string
  icon: ReactNode
  value: number
  detail: string
  to: string
  search?: Record<string, string>
  testId: string
  rule: string
  action: string
}) {
  return (
    <Link
      to={to as LinkTo}
      search={search as never}
      data-testid={testId}
      className="flex min-w-0 flex-col gap-1 p-[18px] hover:bg-primary-tint"
      style={{
        flex: '1 1 200px',
        border: '1px solid var(--rule)',
        borderLeft: `4px solid ${rule}`,
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
        color: 'var(--ink)',
      }}
    >
      <span
        className="inline-flex items-center gap-2"
        style={{ fontSize: 13, color: 'var(--ink-2)' }}
      >
        {icon}
        {label}
      </span>
      <span className="type-display tabular">{value}</span>
      <span className="type-note" style={{ color: 'var(--ink-3)' }}>
        {detail}
      </span>
      <span
        className="mt-1 inline-flex items-center gap-1.5 font-semibold"
        style={{ fontSize: 13, color: 'var(--primary-ink)' }}
      >
        {action}
        <ArrowRight aria-hidden size={15} strokeWidth={2.5} style={{ flex: 'none' }} />
      </span>
    </Link>
  )
}

/**
 * Spec 7.1 — the ops home. Queue counts: requests awaiting review, open
 * demands, unverified records.
 */
export function OpsHomeScreen() {
  const { t } = useTranslation()
  const { data, isLoading, error, refetch } = useOpsHome()

  if (error) return <ErrorState error={error} onRetry={() => void refetch()} />

  if (isLoading || !data) {
    return <Loading testId="ops-home-loading" />
  }

  return (
    <section data-testid="ops-home" className="flex flex-col gap-3.5">
      <PageHeader title={t('opsHome.title')} description={t('opsHome.lead')} />

      <div className="flex flex-wrap gap-3.5">
        {resourcesEnabled && (
          <Queue
            testId="ops-queue-requests"
            icon={<ClipboardList aria-hidden size={16} strokeWidth={2.25} style={{ flex: 'none' }} />}
            rule="var(--primary)"
            action={t('opsHome.openPipeline')}
            label={t('opsHome.awaitingReview')}
            value={data.awaitingReview}
            detail={t('opsHome.awaitingReviewDetail')}
            to="/ops/requests"
            search={{ status: 'submitted' }}
          />
        )}
        <Queue
          testId="ops-queue-demands"
          icon={<Package aria-hidden size={16} strokeWidth={2.25} style={{ flex: 'none' }} />}
          rule="var(--accent)"
          action={t('opsHome.openOrderBook')}
          label={t('opsHome.openDemands')}
          value={data.openDemands}
          detail={t('opsHome.openDemandsDetail')}
          to="/ops/demand"
        />
        {/* Verification is the officer's job, and ops may read that queue —
            SURFACE_ROLES.officer includes ops and admin. */}
        <Queue
          testId="ops-queue-verification"
          icon={<CircleCheckBig aria-hidden size={16} strokeWidth={2.25} style={{ flex: 'none' }} />}
          rule="var(--ink-3)"
          action={t('opsHome.openVerifyQueue')}
          label={t('opsHome.outstandingRecords')}
          value={data.outstandingRecords}
          detail={t('opsHome.outstandingRecordsDetail')}
          to="/officer/verify"
        />
      </div>
    </section>
  )
}
