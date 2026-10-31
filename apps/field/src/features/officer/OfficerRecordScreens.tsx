import { getRouteApi } from '@tanstack/react-router'
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { ProvenanceBadge } from '@/components/ProvenanceBadge'
import { railColour } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { useCycleDetail, useFarmDetail } from '@/features/officer/useOfficerRecords'
import { RecordEditAction } from '@/features/officer/RecordEditAction'
import { useCrops } from '@/features/officer/useCrops'
import { VerifyButton } from '@/features/officer/VerifyButton'
import { useVerifyFromQueue } from '@/features/officer/useVerifyQueue'
import { formatArea, formatKg, formatPlainDate } from '@/lib/format'

const farmRoute = getRouteApi('/_officer/officer/farms/$farmId')
const cycleRoute = getRouteApi('/_officer/officer/cycles/$cycleId')

/**
 * Officer detail screens keep the full provenance graph visible while exposing
 * small, section-level correction forms. Server RPCs remain authoritative.
 */

function Detail({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div
      className="flex min-w-0 flex-col gap-0.5 px-3.5 py-2.5"
      style={{
        flex: '1 1 160px',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-control)',
        background: 'var(--sand-2)',
      }}
    >
      <p className="type-note" style={{ color: 'var(--ink-2)' }}>
        {label}
      </p>
      <p className="tabular" style={{ fontSize: 15, fontWeight: 500 }}>
        {value}
      </p>
    </div>
  )
}

function farmFields() {
  return [
    { name: 'label', label: 'officerEdit.fields.farm_label', required: true },
    { name: 'latitude', label: 'officerEdit.fields.latitude', type: 'number' as const, step: 'any' },
    { name: 'longitude', label: 'officerEdit.fields.longitude', type: 'number' as const, step: 'any' },
  ]
}

function plotFields() {
  return [
    { name: 'label', label: 'officerEdit.fields.plot_label', required: true },
    { name: 'area_ha', label: 'officerEdit.fields.plot_area_ha', type: 'number' as const, step: 'any' },
    { name: 'latitude', label: 'officerEdit.fields.latitude', type: 'number' as const, step: 'any' },
    { name: 'longitude', label: 'officerEdit.fields.longitude', type: 'number' as const, step: 'any' },
  ]
}

function cycleFields(crops: Array<{ id: string; name: string; measured_by: 'area' | 'tree_count' | 'unit_count' }>) {
  return [
    { name: 'crop_id', label: 'officerEdit.fields.crop_id', options: crops.map((crop) => ({ value: crop.id, label: crop.name })) },
    { name: 'season_label', label: 'officerEdit.fields.season_label' },
    { name: 'area_ha', label: 'officerEdit.fields.cycle_area_ha', type: 'number' as const, step: 'any' },
    { name: 'tree_count', label: 'officerEdit.fields.tree_count', type: 'number' as const, step: '1' },
    { name: 'unit_count', label: 'officerEdit.fields.unit_count', type: 'number' as const, step: '1' },
    { name: 'planted_on', label: 'officerEdit.fields.planted_on', type: 'date' as const },
    { name: 'harvest_start', label: 'officerEdit.fields.harvest_start', type: 'date' as const },
    { name: 'harvest_end', label: 'officerEdit.fields.harvest_end', type: 'date' as const },
    { name: 'status', label: 'officerEdit.fields.status', options: ['planned', 'growing', 'harvested', 'abandoned'].map((value) => ({ value, label: `cycleStatus.${value}` })) },
  ]
}

function harvestFields() {
  return [
    { name: 'quantity_kg', label: 'officerEdit.fields.quantity_kg', type: 'number' as const, step: 'any', required: true },
    { name: 'reported_for', label: 'officerEdit.fields.reported_for', type: 'date' as const },
    { name: 'confidence', label: 'officerEdit.fields.confidence', options: [{ value: 'low', label: 'confidence.low' }, { value: 'medium', label: 'confidence.medium' }, { value: 'high', label: 'confidence.high' }] },
  ]
}

export function OfficerFarmScreen() {
  const { t } = useTranslation()
  const { farmId } = farmRoute.useParams()
  const { plot: focusedPlot } = farmRoute.useSearch()
  const query = useFarmDetail(farmId)
  const verify = useVerifyFromQueue()

  useEffect(() => {
    if (!focusedPlot) return
    const element = document.querySelector(`[data-record-id="${focusedPlot}"]`)
    element?.scrollIntoView?.({ block: 'center' })
  }, [focusedPlot, query.data?.plots])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return (
      <Loading testId="farm-detail-loading" />
    )
  }

  const farm = query.data
  // Zero rows is an answer: the farm does not exist, or RLS puts it outside
  // this officer's villages. Never an error.
  if (!farm) {
    return <EmptyState title={t('farmDetail.notFoundTitle')} detail={t('farmDetail.notFoundDetail')} />
  }

  return (
    <section data-testid="farm-detail" className="flex w-full flex-col gap-4">
      <header className="flex flex-wrap items-center gap-2.5">
        <h1 className="type-screen-title">{farm.label}</h1>
        <ProvenanceBadge
          source={farm.source}
          verification={farm.verification}
          confidence={farm.confidence ?? undefined}
          capturedAt={farm.captured_at}
        />
        <VerifyButton
          table="farm"
          id={farm.id}
          recordLabel={farm.label}
          verification={farm.verification}
          pending={verify.isPending && verify.variables?.table === 'farm' && verify.variables?.id === farm.id}
          onVerify={(target) => verify.mutateAsync(target)}
        />
        <RecordEditAction
          table="farm"
          id={farm.id}
          title="officerEdit.titles.farm"
          fields={farmFields()}
          initialValues={{ label: farm.label, latitude: farm.latitude?.toString() ?? '', longitude: farm.longitude?.toString() ?? '' }}
          context={{ villageId: farm.village_id, farmId: farm.id }}
          version={farm.captured_at}
        />
      </header>

      <div className="flex flex-wrap gap-2.5">
        <Detail
          label={t('farmDetail.gps')}
          value={
            farm.latitude !== null && farm.longitude !== null
              ? `${farm.latitude}, ${farm.longitude}`
              : t('farmDetail.noGps')
          }
        />
        <Detail label={t('farmDetail.plotCount')} value={farm.plots.length} />
      </div>

      <div className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('farmDetail.plots')}
        </h2>
        {farm.plots.length === 0 ? (
          <EmptyState title={t('farmDetail.noPlotsTitle')} detail={t('farmDetail.noPlotsDetail')} />
        ) : (
          <ul className="flex flex-col gap-2.5">
            {farm.plots.map((plot) => (
              <li
                key={plot.id}
                data-testid="farm-plot"
                data-record-id={plot.id}
                data-focused={focusedPlot === plot.id ? 'true' : 'false'}
                className="flex flex-col gap-2 p-4"
                style={{
                  border: '1px solid var(--rule)',
                  borderLeft: `3px solid ${railColour(plot.verification)}`,
                  borderRadius: 'var(--radius-card)',
                  background: 'var(--paper)',
                  ...(focusedPlot === plot.id ? { outline: '2px solid var(--primary)' } : {}),
                }}
              >
                <p className="flex flex-wrap items-baseline gap-2.5">
                  <span style={{ fontSize: 16, fontWeight: 600 }}>{plot.label}</span>
                  <span className="tabular" style={{ fontSize: 14, color: 'var(--ink-2)' }}>
                    {formatArea(plot.area_ha, 'hectare')}
                  </span>
                </p>
                <ProvenanceBadge
                  compact
                  source={plot.source}
                  verification={plot.verification}
                  confidence={plot.confidence ?? undefined}
                  capturedAt={plot.captured_at}
                />
                <div className="flex flex-wrap gap-2.5">
                  <VerifyButton
                    table="plot"
                    id={plot.id}
                    recordLabel={plot.label}
                    verification={plot.verification}
                    pending={verify.isPending && verify.variables?.table === 'plot' && verify.variables?.id === plot.id}
                    onVerify={(target) => verify.mutateAsync(target)}
                  />
                  <RecordEditAction
                    table="plot"
                    id={plot.id}
                    title="officerEdit.titles.plot"
                    fields={plotFields()}
                    initialValues={{ label: plot.label, area_ha: plot.area_ha?.toString() ?? '', latitude: plot.latitude?.toString() ?? '', longitude: plot.longitude?.toString() ?? '' }}
                    context={{ villageId: farm.village_id, farmId: farm.id }}
                    version={plot.captured_at}
                  />
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}

export function OfficerCycleScreen() {
  const { t } = useTranslation()
  const { cycleId } = cycleRoute.useParams()
  const { harvest: focusedHarvest } = cycleRoute.useSearch()
  const query = useCycleDetail(cycleId)
  const crops = useCrops()
  const verify = useVerifyFromQueue()

  useEffect(() => {
    if (!focusedHarvest) return
    const element = document.querySelector(`[data-record-id="${focusedHarvest}"]`)
    element?.scrollIntoView?.({ block: 'center' })
  }, [focusedHarvest, query.cycle?.harvests])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return (
      <Loading testId="cycle-detail-loading" />
    )
  }

  const cycle = query.cycle
  if (!cycle) {
    return (
      <EmptyState title={t('cycleDetail.notFoundTitle')} detail={t('cycleDetail.notFoundDetail')} />
    )
  }

  // The measure follows crop.measured_by, so exactly one of these is set.
  const measure =
    cycle.area_ha !== null
      ? formatArea(cycle.area_ha, 'hectare')
      : cycle.tree_count !== null
        ? t('cycleDetail.trees', { count: cycle.tree_count })
        : cycle.unit_count !== null
          ? t('cycleDetail.units', { count: cycle.unit_count })
          : '—'

  return (
    <section data-testid="cycle-detail" className="space-y-5">
      <header className="flex flex-wrap items-center gap-2.5">
        <h1 className="type-screen-title">
          {cycle.crop_name}
          {cycle.plot_label && (
            <span style={{ fontWeight: 400, color: 'var(--ink-2)' }}> · {cycle.plot_label}</span>
          )}
        </h1>
        <ProvenanceBadge
          source={cycle.source}
          verification={cycle.verification}
          confidence={cycle.confidence ?? undefined}
          capturedAt={cycle.captured_at}
        />
        <VerifyButton
          table="crop_cycle"
          id={cycle.id}
          recordLabel={cycle.crop_name}
          verification={cycle.verification}
          pending={verify.isPending && verify.variables?.table === 'crop_cycle' && verify.variables?.id === cycle.id}
          onVerify={(target) => verify.mutateAsync(target)}
        />
        <RecordEditAction
          table="crop_cycle"
          id={cycle.id}
          title="officerEdit.titles.crop_cycle"
          fields={cycleFields(crops.crops)}
          initialValues={{ crop_id: cycle.crop_id, season_label: cycle.season_label ?? '', area_ha: cycle.area_ha?.toString() ?? '', tree_count: cycle.tree_count?.toString() ?? '', unit_count: cycle.unit_count?.toString() ?? '', planted_on: cycle.planted_on ?? '', harvest_start: cycle.harvest_start ?? '', harvest_end: cycle.harvest_end ?? '', status: cycle.status }}
          measure={crops.crops.find((crop) => crop.id === cycle.crop_id)?.measured_by}
          measureByCrop={Object.fromEntries(crops.crops.map((crop) => [crop.id, crop.measured_by]))}
          context={{ villageId: cycle.village_id, cycleId: cycle.id }}
          version={cycle.captured_at}
        />
      </header>

      <div className="flex flex-wrap gap-2.5">
        <Detail label={t('cycleDetail.measure')} value={measure} />
        <Detail label={t('cycleDetail.status')} value={t(`cycleStatus.${cycle.status}`)} />
        <Detail
          label={t('cycleDetail.window')}
          value={`${formatPlainDate(cycle.harvest_start)} – ${formatPlainDate(cycle.harvest_end)}`}
        />
        {/* Free text until a season taxonomy is agreed (S22). Shown verbatim. */}
        <Detail label={t('cycleDetail.season')} value={cycle.season_label ?? '—'} />
      </div>

      <div className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('cycleDetail.harvests')}
        </h2>
        {/* A harvest figure is a series, not a value: superseded rows stay
            visible and labelled so a revised estimate has an audit trail. */}
        <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {t('cycleDetail.seriesNote')}
        </p>

        {cycle.harvests.length === 0 ? (
          <EmptyState
            title={t('cycleDetail.noHarvestsTitle')}
            detail={t('cycleDetail.noHarvestsDetail')}
          />
        ) : (
          <ul className="flex flex-col gap-2.5">
            {cycle.harvests.map((h) => (
              <li
                key={h.id}
                data-testid="cycle-harvest"
                data-current={h.is_current}
                data-record-id={h.id}
                data-focused={focusedHarvest === h.id ? 'true' : 'false'}
                className="flex flex-col gap-2 p-4"
                style={{
                  /*
                    The current figure is outlined in blue; a superseded one is
                    greyed and struck through. The 3,200 kg the Tower excludes
                    stays visible and is obviously not counted.
                  */
                  border: h.is_current
                    ? '1.5px solid var(--primary)'
                    : '1px solid var(--rule-2)',
                  borderRadius: 'var(--radius-card)',
                  background: h.is_current ? 'var(--paper)' : 'var(--sand-2)',
                  ...(focusedHarvest === h.id ? { outline: '2px solid var(--primary)' } : {}),
                }}
              >
                <p className="flex flex-wrap items-baseline gap-2.5">
                  <span
                    className="type-note px-2 py-0.5"
                    style={{
                      border: '1px solid var(--rule-2)',
                      borderRadius: 'var(--radius-pill)',
                      background: 'var(--paper)',
                      color: 'var(--ink-2)',
                    }}
                  >
                    {t(`harvestKind.${h.kind}`)}
                  </span>
                  <span
                    className="tabular font-semibold"
                    style={{
                      fontSize: 17,
                      ...(h.is_current
                        ? {}
                        : { textDecoration: 'line-through', color: 'var(--ink-2)' }),
                    }}
                  >
                    {formatKg(h.quantity_kg)}
                  </span>
                  {!h.is_current && (
                    <span className="type-note" style={{ color: 'var(--ink-3)' }}>
                      {t('cycleDetail.superseded')}
                    </span>
                  )}
                </p>
                <ProvenanceBadge
                  compact
                  source={h.source}
                  verification={h.verification}
                  confidence={h.confidence ?? undefined}
                  capturedAt={h.captured_at}
                />
                <div className="flex flex-wrap gap-2.5">
                  <VerifyButton
                    table="harvest_report"
                    id={h.id}
                    recordLabel={`${h.kind} ${formatKg(h.quantity_kg)}`}
                    verification={h.verification}
                    pending={verify.isPending && verify.variables?.table === 'harvest_report' && verify.variables?.id === h.id}
                    onVerify={(target) => verify.mutateAsync(target)}
                  />
                  {h.is_current && (
                    <RecordEditAction
                      table="harvest_report"
                      id={h.id}
                      title="officerEdit.titles.harvest_report"
                      fields={harvestFields()}
                      initialValues={{ quantity_kg: h.quantity_kg.toString(), reported_for: h.reported_for ?? '', confidence: h.confidence ?? 'medium' }}
                      context={{ villageId: cycle.village_id, cycleId: cycle.id, harvestKind: h.kind }}
                      version={h.captured_at}
                    />
                  )}
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}
