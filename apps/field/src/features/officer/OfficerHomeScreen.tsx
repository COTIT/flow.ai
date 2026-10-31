import { Link } from '@tanstack/react-router'
import { UserPlus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Loading } from '@/components/controls'
import { VerificationMark } from '@/components/marks'
import { useOfficerHome } from '@/features/officer/useOfficerHome'
import { resourcesEnabled } from '@/lib/features'

type LinkTo = Parameters<typeof Link>[0]['to']

function Figure({ label, value }: { label: string; value: number }) {
  return (
    <div
      className="flex min-w-0 flex-col gap-0.5 px-3 py-2.5"
      style={{
        flex: '1 1 0',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-control)',
        background: 'var(--sand-2)',
      }}
    >
      <p className="type-note" style={{ color: 'var(--ink-2)' }}>
        {label}
      </p>
      <p className="tabular" style={{ fontSize: 22, fontWeight: 600 }}>
        {value}
      </p>
    </div>
  )
}

/**
 * Spec 5.1 — the officer's home, and the first screen an officer sees.
 *
 * Assigned villages, their record counts, what still needs verifying, and
 * Register as the primary action. Read-only: everything that writes lives one
 * tap away, because the officer's job starts with Register and this screen's
 * job is to get them there.
 */
export function OfficerHomeScreen() {
  const { t } = useTranslation()
  const { villages, isLoading, error, refetch } = useOfficerHome()

  if (error) return <ErrorState error={error} onRetry={() => void refetch()} />

  if (isLoading) {
    return (
      <Loading testId="officer-home-loading" />
    )
  }

  // Zero assigned villages: a real state with a next step, never a blank page.
  // An officer added to the project but not to a village cannot register
  // anyone, so the action is deliberately not offered here.
  if (villages.length === 0) {
    return (
      <section data-testid="officer-home" className="space-y-4">
        <h1 className="text-lg font-semibold">{t('officerHome.title')}</h1>
        <EmptyState
          title={t('officerHome.noVillagesTitle')}
          detail={t('officerHome.noVillagesDetail')}
        />
      </section>
    )
  }

  const outstanding = villages.reduce((sum, v) => sum + v.unverified, 0)

  return (
    <section data-testid="officer-home" className="flex flex-col gap-4">
      <header className="flex flex-col gap-2.5">
        <h1 className="type-screen-title">{t('officerHome.title')}</h1>
        {/* The officer's one recurring task, as a target rather than a link. */}
        <Link
          to={'/officer/register' as LinkTo}
          data-testid="officer-home-register"
          className="flex w-full items-center justify-center gap-2.5 font-semibold"
          style={{
            minHeight: 52,
            borderRadius: 'var(--radius-control)',
            background: 'var(--primary)',
            color: '#fff',
            fontSize: 17,
            textWrap: 'balance',
          }}
        >
          <UserPlus aria-hidden size={19} strokeWidth={2.25} style={{ flex: 'none' }} />
          {t('officerHome.register')}
        </Link>
      </header>

      {/* Verification is a deliberate act (business-rules §5), so the
          outstanding figure is a prompt to go and do it rather than a passive
          statistic. */}
      <div
        data-testid="officer-unverified"
        className="flex flex-col gap-2.5 p-4"
        style={{
          border: '1px solid var(--rule)',
          borderLeft: '4px solid var(--primary)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--paper)',
        }}
      >
        {outstanding === 0 ? (
          <p style={{ fontSize: 15 }}>{t('officerHome.nothingOutstanding')}</p>
        ) : (
          <>
            <p className="inline-flex items-center gap-2.5" style={{ fontSize: 15, fontWeight: 500 }}>
              <VerificationMark verification="unverified" size={18} />
              {t('officerHome.outstanding', { count: outstanding })}
            </p>
            <Link
              to={'/officer/verify' as LinkTo}
              data-testid="officer-unverified-link"
              className="flex w-full items-center justify-center font-semibold"
              style={{
                minHeight: 48,
                border: '1.5px solid var(--primary)',
                borderRadius: 'var(--radius-control)',
                background: 'var(--primary-tint)',
                color: 'var(--primary-ink)',
                fontSize: 15,
              }}
            >
              {t('officerHome.openVerifyQueue')}
            </Link>
          </>
        )}
      </div>

      <div className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('officerHome.villages')}
        </h2>
        <ul className="flex flex-col gap-2.5">
          {villages.map((v) => (
            <li
              key={v.villageId}
              data-testid="officer-village"
              className="flex flex-col gap-3 p-4"
              style={{
                border: '1px solid var(--rule)',
                borderRadius: 'var(--radius-card)',
                background: 'var(--paper)',
              }}
            >
              <p style={{ fontSize: 17, fontWeight: 600 }}>{v.villageName}</p>
              <div className="flex gap-2.5">
                <Figure label={t('officerHome.people')} value={v.persons} />
                <Figure label={t('officerHome.farms')} value={v.farms} />
                {resourcesEnabled && (
                  <Figure label={t('officerHome.requests')} value={v.requests} />
                )}
              </div>
              {v.unverified > 0 && (
                <p className="type-note" style={{ color: 'var(--ink-3)' }}>
                  {t('officerHome.villageOutstanding', { count: v.unverified })}
                </p>
              )}
            </li>
          ))}
        </ul>
      </div>
    </section>
  )
}
