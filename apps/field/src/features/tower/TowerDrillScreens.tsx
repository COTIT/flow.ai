import type { ReactNode } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useScopeNames } from '@/app/scope'
import { DrillLink } from '@/components/DrillLink'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { TableCard } from '@/components/controls'
import { TileSkeleton } from '@/features/tower/TowerTile'
import { partitionEnergyRows, type EnergyRow } from '@/features/tower/energyRows'
import { validateVillageSearch } from '@/features/tower/towerSearch'
import {
  useTowerEnergy,
  useTowerEnergyRows,
  useTowerMarket,
  useTowerProduction,
  useTowerProductionRows,
} from '@/features/tower/useTower'
import { formatArea, formatKg, formatKw, formatPercent, formatPlainDate } from '@/lib/format'

/**
 * Spec 8.2 — each drill-down is a table that ends in a link to an actual row.
 *
 * "If a number cannot be traced, it does not belong on the screen." So every
 * row here carries a DrillLink to the record underneath it.
 */

const productionRoute = getRouteApi('/_ops/ops/tower/production')
const energyRoute = getRouteApi('/_ops/ops/tower/energy')
const marketRoute = getRouteApi('/_ops/ops/tower/market')

function DrillHeader({
  village,
  kind,
  title,
}: {
  village: string | undefined
  kind: string
  title: string
}) {
  const { t } = useTranslation()
  const scope = useScopeNames()
  const villageName = village ? (scope.data?.villages?.[village] ?? '') : ''

  return (
    <PageHeader
      backTo="/ops/tower"
      backSearch={{ village }}
      backLabel={t('tower.backToTower')}
      eyebrow={[villageName, kind].filter(Boolean).join(' · ')}
      title={title}
      breadcrumbs={[{ label: t('nav.tower'), to: '/ops/tower' }, { label: title }]}
    />
  )
}

function Th({ children, numeric = false }: { children: ReactNode; numeric?: boolean }) {
  return (
    <th
      scope="col"
      className={numeric ? 'type-column-label px-3.5 py-2.5 text-right' : 'type-column-label px-3.5 py-2.5 text-left'}
      style={{ color: 'var(--ink-3)' }}
    >
      {children}
    </th>
  )
}

export function TowerProductionScreen() {
  const { t } = useTranslation()
  const { village } = validateVillageSearch(productionRoute.useSearch())
  const totals = useTowerProduction(village)
  const cycles = useTowerProductionRows(village)

  const error = totals.error ?? cycles.error
  if (error) return <ErrorState error={error} onRetry={() => void cycles.refetch()} />

  const grouped = totals.data ?? []
  // The tile's own headline, restated at the foot of the rows that make it up.
  const leading = grouped.reduce<(typeof grouped)[number] | undefined>(
    (best, row) => ((row.expected_kg ?? 0) > (best?.expected_kg ?? -1) ? row : best),
    undefined,
  )

  return (
    <section className="flex w-full flex-col gap-4">
      <DrillHeader village={village} kind={t('tower.production')} title={t('tower.productionDrill')} />

      {/* The grouped figures the tile shows, so the drill-down and the
          headline are visibly the same numbers. */}
      {!totals.isLoading && grouped.length > 0 && (
        <ul className="flex flex-wrap gap-2">
          {grouped.map((row) => (
            <li
              key={`${row.crop_id}-${row.window_month}`}
              className="inline-flex items-center gap-2 px-3 py-1.5"
              style={{
                border: '1px solid var(--rule-2)',
                borderRadius: 'var(--radius-pill)',
                background: 'var(--paper)',
                fontSize: 13,
                color: 'var(--ink-2)',
              }}
            >
              {row.crop_name} · {formatPlainDate(row.window_month)}
              <b className="tabular font-semibold" style={{ color: 'var(--ink)' }}>
                {formatKg(row.expected_kg)}
              </b>
            </li>
          ))}
        </ul>
      )}

      {cycles.isLoading ? (
        <TileSkeleton rows={4} />
      ) : (cycles.data ?? []).length === 0 ? (
        <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
      ) : (
        <TableCard>
          <table
            data-testid="production-table"
            className="w-full border-collapse"
            style={{ fontSize: 15 }}
          >
            <thead>
              <tr style={{ background: 'var(--sand-2)' }}>
                <Th>{t('tower.colCropFarmerPlot')}</Th>
                <Th>{t('tower.colWindow')}</Th>
                <Th numeric>{t('tower.colPlanted')}</Th>
                <Th numeric>{t('tower.colExpected')}</Th>
                <Th numeric>{t('tower.colActual')}</Th>
              </tr>
            </thead>
            <tbody>
              {(cycles.data ?? []).map((row) => (
                <tr
                  key={row.id}
                  data-testid="production-row"
                  style={{ borderTop: '1px solid var(--rule)' }}
                >
                  <td className="px-3.5 py-3">
                    <span className="flex flex-col gap-0.5">
                      {/*
                        The testid sits on the cycle link alone, not on the cell:
                        the farmer's name is a second drill in the same cell now,
                        and `production-cycle` has always meant the way to the
                        crop cycle rather than the way to anything in that column.
                      */}
                      <span data-testid="production-cycle">
                        <DrillLink kind="cycle" id={row.id}>
                          {row.crop_name}
                        </DrillLink>
                      </span>
                      <span className="type-note" style={{ color: 'var(--ink-2)' }}>
                        <DrillLink kind="person" id={row.person_id}>
                          {row.farmer ?? '—'}
                        </DrillLink>{' '}
                        · {row.plot_label ?? '—'}
                      </span>
                    </span>
                  </td>
                  <td className="tabular px-3.5 py-3" style={{ color: 'var(--ink-2)' }}>
                    {formatPlainDate(row.harvest_start)} – {formatPlainDate(row.harvest_end)}
                  </td>
                  <td className="tabular px-3.5 py-3 text-right">
                    {formatArea(row.area_ha, 'hectare')}
                  </td>
                  <td className="tabular px-3.5 py-3 text-right font-semibold">
                    {formatKg(row.expected_kg)}
                  </td>
                  <td className="tabular px-3.5 py-3 text-right">{formatKg(row.actual_kg)}</td>
                </tr>
              ))}

              {leading && (
                <tr
                  style={{ borderTop: '1px solid var(--rule-2)', background: 'var(--sand-2)' }}
                >
                  <td colSpan={3} className="px-3.5 py-3" style={{ fontSize: 13, color: 'var(--ink-2)' }}>
                    {t('tower.figureOnTile', {
                      crop: leading.crop_name,
                      window: formatPlainDate(leading.window_month),
                    })}
                  </td>
                  <td className="tabular px-3.5 py-3 text-right font-semibold">
                    {formatKg(leading.expected_kg)}
                  </td>
                  <td className="tabular px-3.5 py-3 text-right">{formatKg(leading.actual_kg)}</td>
                </tr>
              )}
            </tbody>
          </table>
        </TableCard>
      )}
      <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
        {t('tower.plantedAreaNote')}
      </p>
    </section>
  )
}

/**
 * One energy figure and the rows behind it, in a card of its own.
 *
 * Two grouped `<tbody>` blocks in one table became two separate tables in two
 * cards. Sharing a table meant sharing a column, and a column of figures invites
 * being added up — which is the one thing these two totals must never be.
 *
 * `rawKw` and `peakKw` both come from `v_village_energy` — the sum is NOT
 * computed from the rows. Business-rules §11 puts village aggregates in the
 * database, and showing a client-side total here would be a second source of
 * truth that could disagree with the tile the user just clicked.
 */
function EnergyGroup({
  testId,
  tableTestId,
  title,
  note,
  rows,
  rawKw,
  factor,
  peakKw,
  peakTestId,
  decided = false,
}: {
  testId: string
  tableTestId?: string
  title: string
  note: string
  rows: EnergyRow[]
  rawKw: number | null
  factor: number | null
  peakKw: number | null
  peakTestId: string
  decided?: boolean
}) {
  const { t } = useTranslation()
  const ink = decided ? 'var(--primary-ink)' : 'var(--ink-2)'

  return (
    <section
      data-testid={testId}
      className="flex flex-col overflow-hidden"
      style={{
        flex: '1 1 340px',
        minWidth: 0,
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      <header
        className="flex flex-col gap-1 px-[18px] py-3.5"
        style={{
          background: decided ? 'var(--primary-tint)' : 'var(--paper)',
          borderBottom: '1px solid var(--rule)',
        }}
      >
        <h2 style={{ fontSize: 15, fontWeight: 600, color: decided ? 'var(--primary-ink)' : undefined }}>
          {title}
        </h2>
        <p className="type-note" style={{ color: ink, textWrap: 'pretty' }}>
          {note}
        </p>
      </header>

      <div className="overflow-x-auto">
        <table data-testid={tableTestId} className="w-full border-collapse" style={{ fontSize: 14 }}>
          <thead>
            <tr style={{ background: 'var(--sand-2)' }}>
              <Th>{t('tower.colApplicantEquipment')}</Th>
              <Th numeric>{t('tower.colPeak')}</Th>
            </tr>
          </thead>

          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td colSpan={2} className="px-[18px] py-3.5" style={{ color: 'var(--ink-2)' }}>
                  {t('tower.noneInFigure')}
                </td>
              </tr>
            ) : (
              rows.map((row) => (
                <tr key={row.id} data-testid="energy-row" style={{ borderTop: '1px solid var(--rule)' }}>
                  <td className="px-[18px] py-3">
                    <span className="flex flex-col items-start gap-1">
                      <b style={{ fontSize: 15, fontWeight: 500 }}>{row.applicant}</b>
                      <span style={{ fontSize: 13, color: 'var(--ink-2)' }}>
                        {row.equipment_name}
                      </span>
                      <StatusPill kind="request" status={row.status} />
                    </span>
                  </td>
                  <td className="px-[18px] py-3 text-right align-top">
                    {/* Ends in the request itself. */}
                    <DrillLink kind="request" id={row.id}>
                      {formatKw(row.est_power_kw)}
                    </DrillLink>
                  </td>
                </tr>
              ))
            )}
          </tbody>

          {/* The arithmetic stated on screen, so the rows above visibly
              reconcile with the headline the user clicked to get here. */}
          <tfoot>
            <tr
              style={{
                borderTop: '1px solid var(--rule-2)',
                background: decided ? 'var(--primary-tint)' : 'var(--sand-2)',
              }}
            >
              <td className="px-[18px] py-3" style={{ fontSize: 13, color: ink }}>
                {t('tower.sumOfPeaks')}{' '}
                <b className="tabular font-semibold" style={{ color: decided ? undefined : 'var(--ink)' }}>
                  {formatKw(rawKw)}
                </b>
                {factor !== null && <> {t('tower.timesFactor', { factor })}</>}
              </td>
              <td
                data-testid={peakTestId}
                className="tabular px-[18px] py-3 text-right font-semibold"
                style={{ fontSize: 17, color: decided ? 'var(--primary-ink)' : undefined }}
              >
                {formatKw(peakKw)}
              </td>
            </tr>
          </tfoot>
        </table>
      </div>
    </section>
  )
}

export function TowerEnergyScreen() {
  const { t } = useTranslation()
  const { village } = validateVillageSearch(energyRoute.useSearch())
  const rows = useTowerEnergyRows(village)
  const totals = useTowerEnergy(village)

  const error = rows.error ?? totals.error
  if (error) return <ErrorState error={error} onRetry={() => void rows.refetch()} />

  const { prospective, approved, excluded } = partitionEnergyRows(rows.data ?? [])
  const energy = totals.data
  const isLoading = rows.isLoading || totals.isLoading

  return (
    <section className="flex flex-col gap-[18px]">
      <DrillHeader village={village} kind={t('tower.energy')} title={t('tower.energyDrill')} />

      {isLoading ? (
        <TileSkeleton rows={4} />
      ) : (rows.data ?? []).length === 0 ? (
        <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
      ) : (
        <>
          <div className="flex flex-wrap gap-4">
            <EnergyGroup
              testId="energy-group-prospective"
              tableTestId="energy-table"
              title={t('tower.prospectivePeak')}
              note={t('tower.prospectiveNote')}
              rows={prospective}
              rawKw={energy?.prospective_kw_raw ?? null}
              factor={energy?.simultaneity_factor ?? null}
              peakKw={energy?.prospective_peak_kw ?? null}
              peakTestId="energy-prospective-peak"
            />

            <EnergyGroup
              testId="energy-group-approved"
              title={t('tower.approvedPeak')}
              note={t('tower.approvedNote')}
              rows={approved}
              rawKw={energy?.approved_kw_raw ?? null}
              factor={energy?.simultaneity_factor ?? null}
              peakKw={energy?.approved_peak_kw ?? null}
              peakTestId="energy-approved-peak"
              decided
            />
          </div>

          <div
            className="flex flex-col gap-2 p-4"
            style={{
              border: '1px solid var(--rule)',
              borderRadius: 'var(--radius-card)',
              background: 'var(--paper)',
            }}
          >
            {/* Prospective and approved are never added together. */}
            <p style={{ fontSize: 15, lineHeight: 1.5, fontWeight: 500, textWrap: 'pretty' }}>
              {t('tower.neverSummed')}
            </p>

            {/* Draft, rejected and withdrawn requests feed neither figure. Said
                out loud rather than silently omitted, so the pipeline does not
                look smaller here than on the tile that counts every status. */}
            {excluded.length > 0 && (
              <p
                data-testid="energy-excluded"
                style={{ fontSize: 13, lineHeight: 1.55, color: 'var(--ink-2)', textWrap: 'pretty' }}
              >
                {t('tower.excludedFromFigures', { count: excluded.length })}
              </p>
            )}

            <p className="type-note" style={{ color: 'var(--ink-3)' }}>
              {t('estimate.isEstimate')}
            </p>
          </div>
        </>
      )}
    </section>
  )
}

export function TowerMarketScreen() {
  const { t } = useTranslation()
  const { village } = validateVillageSearch(marketRoute.useSearch())
  const query = useTowerMarket(village)

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  const matches = query.data?.matches ?? []

  return (
    <section className="flex w-full flex-col gap-4">
      <DrillHeader village={village} kind={t('tower.market')} title={t('tower.marketDrill')} />

      {query.isLoading ? (
        <TileSkeleton rows={3} />
      ) : matches.length === 0 ? (
        <EmptyState title={t('demand.noSupplyTitle')} detail={t('demand.noSupplyDetail')} />
      ) : (
        <TableCard>
          <table data-testid="market-table" className="w-full border-collapse" style={{ fontSize: 15 }}>
            <thead>
              <tr style={{ background: 'var(--sand-2)' }}>
                <Th>{t('tower.colBuyerCrop')}</Th>
                <Th numeric>{t('tower.colDemand')}</Th>
                <Th numeric>{t('tower.colAvailable')}</Th>
                <Th numeric>{t('tower.colCoverage')}</Th>
                <Th>{t('tower.colOpportunity')}</Th>
              </tr>
            </thead>
            <tbody>
              {matches.map((row) => (
                <tr
                  key={`${row.buyer_demand_id}-${row.village_id}`}
                  data-testid="market-row"
                  style={{ borderTop: '1px solid var(--rule)' }}
                >
                  <td className="px-3.5 py-3">
                    <span className="flex flex-col gap-0.5">
                      <DrillLink kind="demand" id={row.buyer_demand_id}>
                        {row.buyer_name}
                      </DrillLink>
                      <span className="type-note" style={{ color: 'var(--ink-2)' }}>
                        {row.crop_name} · {formatPlainDate(row.window_start)} –{' '}
                        {formatPlainDate(row.window_end)}
                      </span>
                    </span>
                  </td>
                  <td className="tabular px-3.5 py-3 text-right">{formatKg(row.demand_kg)}</td>
                  <td className="tabular px-3.5 py-3 text-right font-semibold">
                    {formatKg(row.available_kg)}
                  </td>
                  <td className="px-3.5 py-3 text-right">
                    <span className="inline-flex flex-col items-end gap-1">
                      <b className="tabular font-semibold">{formatPercent(row.coverage_pct)}</b>
                      <MiniCoverage pct={row.coverage_pct} />
                    </span>
                  </td>
                  <td data-testid="market-opportunity" className="px-3.5 py-3">
                    {/* Ends in the opportunity's supply lines, which are the
                        records the coverage figure is built from. */}
                    <DrillLink kind="opportunity" id={row.opportunity_id}>
                      {row.opportunity_id
                        ? t(`opportunityStatus.${row.opportunity_status ?? 'proposed'}`)
                        : '—'}
                    </DrillLink>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </TableCard>
      )}
      <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
        {t('demand.notASale')}
      </p>
    </section>
  )
}

/**
 * A 72px bar beside the percentage. The seeded coffee demand has no supply at
 * all, and its empty track is the point: an honest zero, stated rather
 * than left blank.
 */
function MiniCoverage({ pct }: { pct: number | null }) {
  const covered = pct === null ? 0 : Math.min(Math.max(pct, 0), 100)

  return (
    <span
      aria-hidden
      className="flex overflow-hidden"
      style={{
        width: 72,
        height: 8,
        borderRadius: 'var(--radius-pill)',
        border: '1px solid var(--rule)',
        background: 'var(--sand-2)',
      }}
    >
      <span style={{ width: `${covered}%`, background: 'var(--accent)', display: 'block' }} />
    </span>
  )
}
