import type { ReactNode } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useScopeNames } from '@/app/scope'
import { CoverageBar } from '@/components/CoverageBar'
import { DrillLink } from '@/components/DrillLink'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { BUTTON_SECONDARY } from '@/components/controlStyles'
import { IndicativePill, Loading, ProductNote, TableCard } from '@/components/controls'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import {
  useCreateOpportunity,
  useDemand,
  useDemandMatches,
} from '@/features/ops/useDemand'
import { formatKg, formatMoney, formatPercent, formatPlainDate } from '@/lib/format'

const route = getRouteApi('/_ops/ops/demand/$demandId')

/**
 * Spec 7.7 — the comparison, not a match. No allocation, no ranking.
 *
 * Every figure on this screen is read from a view. The client does not
 * aggregate and does not calculate coverage (business-rules §11): coverage_pct
 * comes from v_demand_match and committed_kg from v_village_supply.
 *
 * v_demand_match produces NO ROWS when nothing overlaps, so the zero is
 * STATED rather than left to an empty table. The seeded coffee demand is
 * exactly that case, and hiding it would misrepresent the programme.
 */
export function DemandDetailScreen() {
  const { demandId } = route.useParams()
  const { t } = useTranslation()
  const demandQuery = useDemand(demandId)
  const matchQuery = useDemandMatches(demandId)
  const scope = useScopeNames()
  const createOpportunity = useCreateOpportunity(demandId)

  const error = demandQuery.error ?? matchQuery.error
  if (error) return <ErrorState error={error} onRetry={() => void demandQuery.refetch()} />

  if (demandQuery.isLoading) {
    return (
      <Loading testId="demand-detail-loading" />
    )
  }

  // Zero rows is an answer: RLS says this demand is not visible.
  if (!demandQuery.demand) {
    return <EmptyState title={t('demand.notFoundTitle')} detail={t('demand.notFoundDetail')} />
  }

  const demand = demandQuery.demand
  const matches = matchQuery.data ?? []
  const villageName = (id: string | null) =>
    (id ? scope.data?.villages[id] : undefined) ?? id ?? '—'

  return (
    <section className="flex w-full flex-col gap-[18px]" data-testid="demand-detail">
      <PageHeader
        title={<>{demand.buyer_name} <StatusPill kind="demand" status={demand.status} /></>}
        backTo="/ops/demand"
        backLabel={t('tour.back')}
        breadcrumbs={[{ label: t('nav.demand'), to: '/ops/demand' }, { label: demand.buyer_name }]}
      />
      <header className="flex flex-col gap-2.5">
        <dl className="grid gap-x-5 gap-y-1.5 sm:grid-cols-2">
          <Row label={t('demand.colCrop')} value={demand.crop_name} />
          <Row label={t('demand.colQuantity')} value={formatKg(demand.quantity_kg)} />
          <Row
            label={t('demand.window')}
            value={`${formatPlainDate(demand.window_start)} – ${formatPlainDate(demand.window_end)}`}
          />
          {/* The tag travels with the number rather than trailing it in a
              parenthesis nobody reads. */}
          <Row
            label={t('demand.colPrice')}
            value={formatMoney(demand.indicative_price_per_kg, demand.currency)}
            pill={<IndicativePill />}
          />
          {demand.delivery_point && (
            <Row label={t('demand.deliveryPoint')} value={demand.delivery_point} />
          )}
          {demand.quality_note && <Row label={t('demand.qualityNote')} value={demand.quality_note} />}
        </dl>
      </header>

      <section className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('demand.matches')}
        </h2>

        {matchQuery.isLoading ? (
          <Loading />
        ) : matches.length === 0 ? (
          // An honest zero, stated. The demand above stays on screen.
          <div data-testid="no-matching-supply">
            <EmptyState title={t('demand.noSupplyTitle')} detail={t('demand.noSupplyDetail')} />
          </div>
        ) : (
          <>
            <TableCard>
              <div data-testid="demand-matches" className="overflow-x-auto">
              <table className="w-full border-collapse" style={{ fontSize: 15 }}>
                <thead>
                  <tr style={{ background: 'var(--sand-2)' }}>
                    <Th>{t('demand.colVillage')}</Th>
                    <Th numeric>{t('demand.colAvailable')}</Th>
                    <Th numeric>{t('demand.colCoverable')}</Th>
                    <Th numeric>{t('demand.colCoverage')}</Th>
                    <Th>{t('demand.colOpportunity')}</Th>
                  </tr>
                </thead>
                <tbody>
                  {matches.map((match) => (
                    <tr
                      key={match.village_id}
                      data-testid={`match-row-${match.village_id}`}
                      style={{ borderTop: '1px solid var(--rule)' }}
                    >
                      <td className="px-3.5 py-3" style={{ fontWeight: 500 }}>
                        {villageName(match.village_id)}
                      </td>
                      <td className="tabular px-3.5 py-3 text-right font-semibold">
                        {formatKg(match.available_kg)}
                      </td>
                      <td className="tabular px-3.5 py-3 text-right font-semibold">
                        {formatKg(match.coverable_kg)}
                      </td>
                      <td className="tabular px-3.5 py-3 text-right">
                        {formatPercent(match.coverage_pct)}
                      </td>
                      <td className="px-3.5 py-3">
                        {match.opportunity_id ? (
                          <span data-testid="opportunity-link" className="inline-flex">
                            <DrillLink kind="opportunity" id={match.opportunity_id}>
                              {t(`opportunityStatus.${match.opportunity_status ?? 'proposed'}`)}
                            </DrillLink>
                          </span>
                        ) : (
                          <button
                            type="button"
                            data-testid="create-opportunity"
                            disabled={createOpportunity.isPending}
                            onClick={() =>
                              createOpportunity.mutate({
                                villageId: match.village_id!,
                                cropId: demand.crop_id,
                              })
                            }
                            className="disabled:opacity-60"
                            style={{
                              ...BUTTON_SECONDARY,
                              minHeight: 40,
                              fontSize: 14,
                              borderColor: 'var(--primary)',
                              background: 'var(--primary-tint)',
                              color: 'var(--primary-ink)',
                              fontWeight: 600,
                            }}
                          >
                            {createOpportunity.isPending
                              ? t('demand.creatingOpportunity')
                              : t('demand.createOpportunity')}
                          </button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              </div>
            </TableCard>

            {/* One bar per village: coverage is a per-village figure, and a
                single combined bar would be an aggregate the views never
                produced. */}
            {matches.map((match) => (
              <div
                key={`coverage-${match.village_id}`}
                data-testid={`coverage-${match.village_id}`}
                className="p-4"
                style={{
                  border: '1px solid var(--rule)',
                  borderRadius: 'var(--radius-card)',
                  background: 'var(--paper)',
                }}
              >
                <CoverageBar
                  label={villageName(match.village_id)}
                  demandKg={demand.quantity_kg}
                  availableKg={match.available_kg}
                  committedKg={match.committed_kg}
                  coveragePct={match.coverage_pct}
                />
              </div>
            ))}
          </>
        )}

        {createOpportunity.isError && (
          <div data-testid="create-opportunity-error">
            <ErrorState error={createOpportunity.error} onRetry={() => createOpportunity.reset()} />
          </div>
        )}
      </section>

      {/* An opportunity is not a sale. Stated on every opportunity surface. */}
      <ProductNote>{t('demand.notASale')}</ProductNote>
    </section>
  )
}

function Row({ label, value, pill }: { label: string; value: string; pill?: ReactNode }) {
  return (
    <div
      className="grid items-baseline gap-3"
      style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}
    >
      <dt style={{ fontSize: 13, color: 'var(--ink-2)' }}>{label}</dt>
      <dd className="inline-flex flex-wrap items-baseline justify-end gap-2">
        <span className="tabular font-semibold" style={{ fontSize: 15 }}>
          {value}
        </span>
        {pill}
      </dd>
    </div>
  )
}

function Th({ children, numeric = false }: { children: ReactNode; numeric?: boolean }) {
  return (
    <th
      scope="col"
      className={
        numeric
          ? 'type-column-label px-3.5 py-2.5 text-right'
          : 'type-column-label px-3.5 py-2.5 text-left'
      }
      style={{ color: 'var(--ink-3)' }}
    >
      {children}
    </th>
  )
}
