import { getRouteApi, Link, useNavigate } from '@tanstack/react-router'
import {
  Banknote,
  BatteryCharging,
  ClipboardList,
  LandPlot,
  MapPin,
  Package,
  Sprout,
  Users,
  Weight,
  Zap,
} from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { useScopeNames } from '@/app/scope'
import { CoverageBar } from '@/components/CoverageBar'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import {
  BasisPill,
  Figure,
  Headline,
  RatioMeter,
  TileSkeleton,
  TowerTile,
} from '@/features/tower/TowerTile'
import {
  useTowerEnergy,
  useTowerMarket,
  useTowerPipeline,
  useTowerProduction,
  useTowerQuality,
} from '@/features/tower/useTower'
import { validateVillageSearch } from '@/features/tower/towerSearch'
import {
  formatArea,
  formatKg,
  formatKw,
  formatKwh,
  formatMoney,
  formatPlainDate,
} from '@/lib/format'
import type { Database } from '@/lib/db.types'
import { resourcesEnabled } from '@/lib/features'

const route = getRouteApi('/_ops/ops/tower/')

const MARK = { size: 16, strokeWidth: 2.25, style: { flex: 'none' as const } }

/**
 * Spec 8.1 — the overview: a village selector and five tiles.
 *
 * Built last, from connected records. Every figure comes from a view; the
 * client does not aggregate. The labelling rules on this screen are product
 * requirements rather than copy preferences, so each is stated inline.
 *
 * The tiles are a wrapping flex row rather than a grid, and each declares its
 * own width: Production leads because it carries the most, Data quality is
 * smallest because it counts records rather than reporting a figure. Two-up on
 * a desktop, stacked on a phone, no media query.
 */
export function TowerScreen() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  // Validated at the point of use: the router hands back raw search params.
  const { village } = validateVillageSearch(route.useSearch())
  const scope = useScopeNames()

  const production = useTowerProduction(village)
  const pipeline = useTowerPipeline(village)
  const energy = useTowerEnergy(village)
  const market = useTowerMarket(village)
  const quality = useTowerQuality(village)

  const error =
    production.error ?? pipeline.error ?? energy.error ?? market.error ?? quality.error ?? scope.error
  if (error) return <ErrorState error={error} onRetry={() => void production.refetch()} />

  const search = { village }
  const villageName = village ? (scope.data?.villages?.[village] ?? '') : ''
  const programme = Object.values(scope.data?.projects ?? {})[0] ?? ''

  // The row the headline figure is about. A selection, not an aggregate: the
  // view already grouped by crop and window, and the tile leads with the
  // largest of those rather than inventing a total across them.
  const rows = production.data ?? []
  const leading = rows.reduce<(typeof rows)[number] | undefined>(
    (best, row) => ((row.expected_kg ?? 0) > (best?.expected_kg ?? -1) ? row : best),
    undefined,
  )

  return (
    <section className="flex flex-col gap-5" data-testid="tower">
      <PageHeader
        eyebrow={[villageName, programme].filter(Boolean).join(' · ')}
        title={t('tower.title')}
        description={t('tower.lead')}
        actions={
          <label className="flex min-w-48 flex-col gap-1.5 text-sm font-semibold text-ink-2">
            {t('tower.village')}
            <Select
              value={village ?? ''}
              onValueChange={(value) =>
                void navigate({
                  to: '/ops/tower',
                  search: { village: value || undefined },
                  replace: true,
                })
              }
            >
              <SelectTrigger data-testid="tower-village">
                {village ? villageName : t('tower.chooseVillage')}
              </SelectTrigger>
              <SelectContent>
                {Object.entries(scope.data?.villages ?? {}).map(([id, name]) => (
                  <SelectItem key={id} value={id}>{name}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          </label>
        }
      />

      {!village ? (
        <EmptyState title={t('tower.noVillageTitle')} detail={t('tower.noVillageDetail')} />
      ) : (
        <div className="flex flex-wrap gap-4">
          {/* ── Production · leads, and carries the most ─────────────── */}
          <TowerTile
            id="production"
            testId="tile-production"
            title={t('tower.production')}
            icon={<Sprout aria-hidden {...MARK} />}
            basis="430px"
            drillTo="/ops/tower/production"
            drillSearch={search}
            loading={production.isLoading}
          >
            {production.isLoading ? (
              <TileSkeleton rows={4} />
            ) : rows.length === 0 ? (
              <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
            ) : (
              <>
                <Headline
                  label={t('tower.expectedWeightFor', {
                    crop: leading?.crop_name ?? '',
                    window: formatPlainDate(leading?.window_month ?? null),
                  })}
                  icon={<Weight aria-hidden {...MARK} />}
                  value={formatKg(leading?.expected_kg ?? null)}
                />

                <div className="flex flex-col">
                  <div
                    className="type-column-label grid gap-2.5 pb-[7px]"
                    style={{
                      gridTemplateColumns: 'minmax(0, 1.6fr) repeat(2, minmax(0, 1fr))',
                      borderBottom: '1px solid var(--rule)',
                      color: 'var(--ink-3)',
                    }}
                  >
                    <span>{t('tower.colCropWindow')}</span>
                    <span className="text-right">{t('tower.colExpectedKg')}</span>
                    <span className="text-right">{t('tower.colActualKg')}</span>
                  </div>
                  {rows.map((row) => (
                    <div
                      key={`${row.crop_id}-${row.window_month}`}
                      className="grid items-baseline gap-2.5 py-[11px]"
                      style={{
                        gridTemplateColumns: 'minmax(0, 1.6fr) repeat(2, minmax(0, 1fr))',
                        borderBottom: '1px solid var(--rule)',
                        fontSize: 15,
                      }}
                    >
                      <span style={{ minWidth: 0 }}>
                        <b style={{ fontWeight: 500 }}>{row.crop_name}</b>
                        <span className="type-note block" style={{ color: 'var(--ink-3)' }}>
                          {formatPlainDate(row.window_month)}
                        </span>
                      </span>
                      <span className="tabular text-right font-semibold">
                        {formatKg(row.expected_kg).replace(' kg', '')}
                      </span>
                      <span
                        className="tabular text-right"
                        style={row.actual_kg === null ? { color: 'var(--ink-3)' } : { fontWeight: 600 }}
                      >
                        {row.actual_kg === null ? '—' : formatKg(row.actual_kg).replace(' kg', '')}
                      </span>
                    </div>
                  ))}
                </div>

                {/* Never "land area": intercropping means cycle areas can
                    exceed the village's hectares. The figure is the leading
                    row's own, the same row the headline is about. */}
                <div style={{ borderTop: '1px solid var(--rule-2)', paddingTop: 14 }}>
                  <Figure
                    label={t('tower.plantedArea')}
                    icon={<LandPlot aria-hidden {...MARK} />}
                    value={formatArea(leading?.cycle_area_ha ?? null, 'hectare')}
                    size={19}
                  />
                </div>
                <p
                  className="type-note"
                  style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}
                >
                  {t('tower.plantedAreaNote')}
                </p>
              </>
            )}
          </TowerTile>

          {/* ── Energy · the tile that has to keep two figures apart ─── */}
          <TowerTile
            id="energy"
            testId="tile-energy"
            title={t('tower.energy')}
            icon={<Zap aria-hidden {...MARK} />}
            basis="350px"
            drillTo="/ops/tower/energy"
            drillSearch={search}
            loading={energy.isLoading}
          >
            {energy.isLoading ? (
              <TileSkeleton rows={4} />
            ) : !energy.data ? (
              <EmptyState title={t('tower.noCapacityTitle')} detail={t('tower.noCapacityDetail')} />
            ) : (
              <>
                {/* Capacity is PLANNED, never measured, and the basis sits
                    inside the figure's own block so nothing can separate the
                    two. */}
                <Headline
                  testId="tower-capacity"
                  label={t('tower.capacity')}
                  icon={<Zap aria-hidden {...MARK} />}
                  value={formatKw(energy.data.capacity_kw)}
                  pill={
                    <BasisPill>
                      {t('tower.basis')}:{' '}
                      {t(`capacityBasis.${energy.data.capacity_basis ?? 'planned'}`)}
                    </BasisPill>
                  }
                />

                {/* Two framed cells with a rule between them. There is no
                    layout here that can add them up. */}
                <div
                  className="flex flex-wrap items-stretch overflow-hidden"
                  style={{ border: '1px solid var(--rule-2)', borderRadius: 'var(--radius-card)' }}
                >
                  <PeakCell
                    label={t('tower.prospectivePeak')}
                    value={formatKw(energy.data.prospective_peak_kw)}
                    note={t('tower.prospectiveNote')}
                    testId="tower-prospective-peak"
                  />
                  <span
                    aria-hidden
                    style={{ flex: '0 0 auto', width: '100%', height: 1, background: 'var(--rule-2)' }}
                  />
                  <PeakCell
                    label={t('tower.approvedPeak')}
                    value={formatKw(energy.data.approved_peak_kw)}
                    note={t('tower.approvedNote')}
                    testId="tower-approved-peak"
                    decided
                  />
                </div>

                <p style={{ fontSize: 13, lineHeight: 1.5, fontWeight: 500, textWrap: 'pretty' }}>
                  {t('tower.neverSummed')}
                </p>

                {/* Approved peak against planned capacity. Prospective is not
                    on this bar: it is an application, not a load. */}
                <div className="flex flex-col gap-2" style={{ borderTop: '1px solid var(--rule)', paddingTop: 14 }}>
                  <Figure
                    label={t('tower.headroomLeft')}
                    icon={<BatteryCharging aria-hidden {...MARK} />}
                    value={formatKw(energy.data.headroom_kw)}
                    testId="tower-headroom"
                    size={22}
                  />
                  <HeadroomBar
                    approvedKw={energy.data.approved_peak_kw}
                    capacityKw={energy.data.capacity_kw}
                  />
                  <div
                    className="flex flex-wrap gap-x-4 gap-y-1.5 type-note"
                    style={{ color: 'var(--ink-2)' }}
                  >
                    <LegendItem
                      swatch={{ background: 'var(--primary)' }}
                      label={t('tower.approvedPeak')}
                      value={formatKw(energy.data.approved_peak_kw)}
                    />
                    <LegendItem
                      swatch={{ background: 'var(--sand-2)', border: '1px solid var(--rule-2)' }}
                      label={t('tower.stillFree')}
                      value={formatKw(energy.data.headroom_kw)}
                    />
                  </div>
                  <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
                    {t('tower.headroomNote')}
                  </p>
                </div>

                {/* The factor sits with the peaks it was applied to. */}
                <div className="flex flex-col gap-2" style={{ borderTop: '1px solid var(--rule)', paddingTop: 14 }}>
                  <Figure
                    label={t('tower.simultaneity')}
                    value={String(energy.data.simultaneity_factor ?? '')}
                    testId="tower-simultaneity"
                  />
                  <Figure
                    label={t('tower.approvedPerWeek')}
                    icon={<BatteryCharging aria-hidden {...MARK} />}
                    value={formatKwh(energy.data.approved_kwh_per_week)}
                  />
                  <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
                    {t('tower.simultaneityNote')}
                  </p>
                </div>
              </>
            )}
          </TowerTile>

          {/* ── Market ────────────────────────────────────────────────── */}
          <TowerTile
            id="market"
            testId="tile-market"
            title={t('tower.market')}
            icon={<Package aria-hidden {...MARK} />}
            basis="340px"
            note={t('tower.marketNote')}
            drillTo="/ops/tower/market"
            drillSearch={search}
            loading={market.isLoading}
          >
            {market.isLoading ? (
              <TileSkeleton rows={3} />
            ) : (market.data?.matches ?? []).length === 0 ? (
              <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
            ) : (
              <div className="flex flex-col gap-5">
                {(market.data?.matches ?? []).map((row) => (
                  <div key={row.buyer_demand_id} className="flex flex-col gap-2.5">
                    <div className="flex flex-col gap-0.5">
                      <p style={{ fontSize: 15, fontWeight: 500 }}>{row.buyer_name}</p>
                      <p className="type-note" style={{ color: 'var(--ink-3)' }}>
                        {[
                          row.crop_name,
                          `${formatPlainDate(row.window_start)} – ${formatPlainDate(row.window_end)}`,
                          t('demandStatus.open').toLowerCase(),
                        ].join(' · ')}
                      </p>
                    </div>
                    {/* No committed figure here: v_demand_match reports on a
                        village's available supply and has no committed total of
                        its own. The demand detail screen, which does, shows it.
                        The note stays either way. */}
                    <CoverageBar
                      demandKg={row.demand_kg}
                      availableKg={row.available_kg}
                      coveragePct={row.coverage_pct}
                    />
                  </div>
                ))}
              </div>
            )}
          </TowerTile>

          {/* ── Equipment pipeline — behind the resource release flag ── */}
          {resourcesEnabled && (
            <TowerTile
              id="pue"
              testId="tile-pue"
              title={t('tower.pue')}
              icon={<ClipboardList aria-hidden {...MARK} />}
              basis="280px"
              drillTo="/ops/requests"
              drillSearch={{ village }}
              loading={pipeline.isLoading}
            >
              {pipeline.isLoading ? (
                <TileSkeleton rows={4} />
              ) : (pipeline.data ?? []).length === 0 ? (
                <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
              ) : (
                <>
                  <div className="flex flex-col">
                    {(pipeline.data ?? []).map((row) => (
                      <div
                        key={`${row.status}-${row.currency}`}
                        className="grid items-center gap-3 py-[9px]"
                        style={{
                          gridTemplateColumns: 'minmax(0, 1fr) auto',
                          borderBottom: '1px solid var(--rule)',
                        }}
                      >
                        <Link
                          to="/ops/requests"
                          search={{ village, status: (row.status ?? 'draft') as RequestStatus }}
                          className="inline-flex min-w-0 flex-wrap items-center gap-2 text-primary-ink underline-offset-2 hover:underline"
                        >
                          <StatusPill
                            kind="request"
                            status={(row.status ?? 'draft') as RequestStatus}
                          />
                          <span className="tabular type-note" style={{ color: 'var(--ink-3)' }}>
                            {t('tower.requestCount', { count: row.request_count ?? 0 })}
                          </span>
                        </Link>
                        <span className="flex flex-col items-end">
                          <span className="tabular font-semibold" style={{ fontSize: 15 }}>
                            {formatMoney(row.indicative_value, row.currency ?? 'TZS')}
                          </span>
                          {/* A rent is not a purchase price: shown apart, per day. */}
                          {typeof row.indicative_rent_per_day === 'number' && (
                            <span data-testid="tile-pue-rent" className="tabular type-note" style={{ color: 'var(--ink-2)' }}>
                              {t('tower.rentPerDay', { value: formatMoney(row.indicative_rent_per_day, row.currency ?? 'TZS') })}
                            </span>
                          )}
                        </span>
                      </div>
                    ))}
                  </div>
                  <p
                    className="type-note inline-flex items-start gap-[7px]"
                    style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}
                  >
                    <Banknote aria-hidden {...MARK} />
                    {t('tower.indicativeValueNote')}
                  </p>
                </>
              )}
            </TowerTile>
          )}

          {/* ── Data quality · smallest, sunken, traceable counts ─────── */}
          <TowerTile
            id="quality"
            testId="tile-quality"
            title={t('tower.quality')}
            icon={<CircleMark />}
            basis="230px"
            sunken
            drillTo="/ops/tower/quality"
            drillSearch={{ village, metric: 'persons' }}
            loading={quality.isLoading}
          >
            {quality.isLoading ? (
              <TileSkeleton rows={3} />
            ) : !quality.data ? (
              <EmptyState title={t('tower.noDataTitle')} detail={t('tower.noDataDetail')} />
            ) : (
              <>
                <div className="flex flex-col gap-3.5">
                  <Link to="/ops/tower/quality" search={{ village, metric: 'persons' }}>
                    <RatioMeter
                      label={t('tower.personsVerified')}
                      icon={<Users aria-hidden {...MARK} />}
                      count={quality.data.persons_verified ?? 0}
                      total={quality.data.persons ?? 0}
                    />
                  </Link>
                  <Link to="/ops/tower/quality" search={{ village, metric: 'farms' }}>
                    <RatioMeter
                      label={t('tower.farmsWithGps')}
                      icon={<MapPin aria-hidden {...MARK} />}
                      count={quality.data.farms_with_gps ?? 0}
                      total={quality.data.farms ?? 0}
                    />
                  </Link>
                  <Link to="/ops/tower/quality" search={{ village, metric: 'cycles' }}>
                    <RatioMeter
                      label={t('tower.cyclesWithEstimate')}
                      icon={<Sprout aria-hidden {...MARK} />}
                      count={quality.data.cycles_with_estimate ?? 0}
                      total={quality.data.cycles ?? 0}
                    />
                  </Link>
                </div>
                <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
                  {t('tower.qualityNote')}
                </p>
              </>
            )}
          </TowerTile>
        </div>
      )}
    </section>
  )
}

type RequestStatus = Database['public']['Enums']['pue_status']

/**
 * One of the two peaks. `data-peak-cell` is what
 * `TowerScreen.test.tsx` reads to prove they are two cells and not one block —
 * the layout is the claim here, not the copy.
 */
function PeakCell({
  label,
  value,
  note,
  testId,
  decided = false,
}: {
  label: string
  value: string
  note: string
  testId: string
  decided?: boolean
}) {
  const ink = decided ? 'var(--primary-ink)' : 'var(--ink-3)'

  return (
    <div
      data-peak-cell={decided ? 'approved' : 'prospective'}
      className="flex flex-col gap-[3px] p-3.5"
      style={{
        flex: '1 1 140px',
        minWidth: 0,
        background: decided ? 'var(--primary-tint)' : 'var(--paper)',
      }}
    >
      <p className="type-note" style={{ color: ink }}>
        {label}
      </p>
      <p
        data-testid={testId}
        className="tabular type-figure"
        style={decided ? { color: 'var(--primary-ink)' } : undefined}
      >
        {value}
      </p>
      <p className="type-note" style={{ color: ink, lineHeight: 1.45, textWrap: 'pretty' }}>
        {note}
      </p>
    </div>
  )
}

/**
 * Approved peak against planned capacity. One segment, because only one of the
 * two peaks is a commitment — prospective requests are not a load and do not
 * belong on a bar that says what is spoken for.
 */
function HeadroomBar({
  approvedKw,
  capacityKw,
}: {
  approvedKw: number | null
  capacityKw: number | null
}) {
  const share =
    approvedKw === null || capacityKw === null || capacityKw <= 0
      ? 0
      : Math.min((approvedKw / capacityKw) * 100, 100)

  return (
    <div
      className="flex w-full overflow-hidden"
      style={{
        height: 18,
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-pill)',
        background: 'var(--sand-2)',
      }}
    >
      <span style={{ width: `${share}%`, minWidth: share > 0 ? 6 : 0, background: 'var(--primary)', display: 'block' }} />
    </div>
  )
}

function LegendItem({
  swatch,
  label,
  value,
}: {
  swatch: React.CSSProperties
  label: string
  value: string
}) {
  return (
    <span className="inline-flex items-center gap-[7px]">
      <span
        aria-hidden
        style={{ width: 10, height: 10, borderRadius: 3, flex: 'none', ...swatch }}
      />
      {label} <b className="tabular font-semibold">{value}</b>
    </span>
  )
}

/** The quality tile's own mark: a ring, because it counts rather than measures. */
function CircleMark() {
  return (
    <span
      aria-hidden
      style={{
        width: 14,
        height: 14,
        borderRadius: 'var(--radius-pill)',
        border: '2px solid var(--ink-3)',
        boxSizing: 'border-box',
        flex: 'none',
      }}
    />
  )
}
