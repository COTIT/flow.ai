import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Loading, ProductNote } from '@/components/controls'
import { StatusPill } from '@/components/StatusPill'
import { useFarmerOpportunities } from '@/features/farmer/useFarmerOpportunities'
import { formatKg } from '@/lib/format'

/**
 * Spec 6.6 — opportunities this farmer's supply is inside.
 *
 * Read-only. An opportunity is created and moved by ops; a farmer sees that
 * their harvest has been put forward, and nothing here implies they agreed to
 * anything or that money exists.
 */
export function FarmerOpportunitiesScreen() {
  const { t } = useTranslation()
  const query = useFarmerOpportunities()

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  const rows = query.data ?? []

  return (
    <section data-testid="farmer-opportunities" className="flex max-w-lg flex-col gap-4">
      <header className="flex flex-col gap-2.5">
        <h1 className="type-screen-title">{t('farmerOpportunities.title')}</h1>
        {/* Not a sale, not a delivery, not a payment. Stated on every
            opportunity screen, farmer-facing most of all — at 14px on a panel,
            where it cannot shrink into fine print. */}
        <ProductNote>
          <span data-testid="farmer-opportunities-note">
            {t('farmerOpportunities.notASale')}
          </span>
        </ProductNote>
      </header>

      {query.isLoading ? (
        <Loading testId="farmer-opportunities-loading" />
      ) : rows.length === 0 ? (
        <EmptyState
          title={t('farmerOpportunities.noneTitle')}
          detail={t('farmerOpportunities.noneDetail')}
        />
      ) : (
        <ul className="flex flex-col gap-3">
          {rows.map((o) => (
            <li
              key={o.id}
              data-testid="farmer-opportunity"
              className="flex flex-col gap-3 p-4"
              style={{
                border: '1px solid var(--rule)',
                borderRadius: 'var(--radius-card)',
                background: 'var(--paper)',
              }}
            >
              <div className="flex flex-wrap items-center justify-between gap-2.5">
                <p style={{ fontSize: 17, fontWeight: 600 }}>{o.crop_name}</p>
                <StatusPill kind="opportunity" status={o.status} />
              </div>

              {/* No buyer, no demand window: `demand_read` is staff-only, so a
                  farmer cannot see who the buyer is. Saying so is better than
                  rendering blanks where a name would go. */}
              <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
                {t('farmerOpportunities.buyerWithOps')}
              </p>

              {/*
                The farmer's own share is the tinted card and the opportunity's
                total is context beside it. On this surface the figure that
                concerns them has to be the one that reads first.
              */}
              <dl className="flex flex-wrap gap-2.5">
                <div
                  className="flex min-w-0 flex-col gap-0.5 px-3.5 py-3"
                  style={{
                    flex: '1 1 150px',
                    border: '1px solid var(--primary)',
                    borderRadius: 'var(--radius-control)',
                    background: 'var(--primary-tint)',
                  }}
                >
                  <dt className="type-note" style={{ color: 'var(--primary-ink)' }}>
                    {t('farmerOpportunities.yourShare')}
                  </dt>
                  <dd
                    data-testid="my-contribution"
                    className="tabular type-figure"
                    style={{ color: 'var(--primary-ink)' }}
                  >
                    {formatKg(o.my_contribution_kg)}
                  </dd>
                </div>
                <div
                  className="flex min-w-0 flex-col gap-0.5 px-3.5 py-3"
                  style={{
                    flex: '1 1 150px',
                    border: '1px solid var(--rule)',
                    borderRadius: 'var(--radius-control)',
                    background: 'var(--sand-2)',
                  }}
                >
                  <dt className="type-note" style={{ color: 'var(--ink-2)' }}>
                    {t('farmerOpportunities.opportunityTotal')}
                  </dt>
                  <dd data-testid="offered-total" className="tabular" style={{ fontSize: 17, fontWeight: 600 }}>
                    {formatKg(o.offered_quantity_kg)}
                  </dd>
                </div>
              </dl>
            </li>
          ))}
        </ul>
      )}
    </section>
  )
}
