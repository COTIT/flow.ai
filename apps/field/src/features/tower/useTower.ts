import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmMarketVillageMatches } from '@genesy/api-client/endpoints/farm-market'
import {
  farmPueRequestsList,
  farmPueVillageEnergy,
  farmPueVillagePipeline,
} from '@genesy/api-client/endpoints/farm-pue'
import {
  farmRecordsCropCyclesList,
  farmRecordsFarmsList,
  farmRecordsHarvestReportsAll,
  farmRecordsPersonsList,
  farmRecordsPlotsList,
} from '@genesy/api-client/endpoints/farm-records'
import { farmTowerDataQuality, farmTowerProduction } from '@genesy/api-client/endpoints/farm-tower'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { useCrops } from '@/features/officer/useCrops'
import { localisedName } from '@/lib/names'
import { queryKeys } from '@/lib/queryKeys'
import type { Enums } from '@/lib/enums'
import type { QualityMetric } from '@/features/tower/towerSearch'


/**
 * The Tower reads the API's village figures and nothing else.
 *
 * Every aggregate is the API's (`farm/villages/{v}/production|energy|pue-pipeline|data-quality|
 * demand-matches`): the client never sums, never derives coverage, and never combines prospective
 * with approved. Decimal strings become numbers for display only, as the other ported screens do.
 * Crop names are chosen at render, so a cached row is never served in the wrong language.
 */
function useLanguage(): string | undefined {
  return useTranslation().i18n.resolvedLanguage
}

const num = (value: string | null | undefined) => (value == null ? null : Number(value))

/** Crop id → name in the active language, from the workspace's crops. */
function useCropNames() {
  const { crops } = useCrops()
  return useMemo(() => new Map(crops.map((c) => [c.id, c.name])), [crops])
}

export function useTowerProduction(villageId: string | undefined) {
  const names = useCropNames()
  const query = useQuery({
    queryKey: queryKeys.tower.production(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => (await farmTowerProduction(workspaceId(), villageId!)).data,
  })

  const data = useMemo(
    () =>
      query.data?.map((r) => ({
        ...r,
        crop_name: names.get(r.crop_id) || r.crop_id,
        // The sum of cycle areas, not land area: intercropped cycles share a plot.
        cycle_area_ha: num(r.cycle_area_ha),
        expected_kg: num(r.expected_kg),
        actual_kg: num(r.actual_kg),
      })),
    [query.data, names],
  )

  return { ...query, data }
}

/** One row per status and currency; values are Money (indicative, never financed value). */
export function useTowerPipeline(villageId: string | undefined) {
  return useQuery({
    queryKey: [...queryKeys.tower.market(villageId ?? ''), 'pue'],
    enabled: Boolean(villageId),
    queryFn: async () => (await farmPueVillagePipeline(workspaceId(), villageId!)).data,
  })
}

export function useTowerEnergy(villageId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.tower.energy(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => {
      const e = (await farmPueVillageEnergy(workspaceId(), villageId!)).data
      return {
        ...e,
        capacity_kw: num(e.capacity_kw),
        simultaneity_factor: num(e.simultaneity_factor),
        prospective_kw_raw: num(e.prospective_kw_raw),
        approved_kw_raw: num(e.approved_kw_raw),
        prospective_peak_kw: num(e.prospective_peak_kw),
        approved_peak_kw: num(e.approved_peak_kw),
        headroom_kw: num(e.headroom_kw),
        approved_kwh_per_week: num(e.approved_kwh_per_week),
      }
    },
  })
}

export function useTowerMarket(villageId: string | undefined) {
  const names = useCropNames()
  const query = useQuery({
    queryKey: queryKeys.tower.market(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => (await farmMarketVillageMatches(workspaceId(), villageId!)).data,
  })

  const data = useMemo(
    () =>
      query.data && {
        matches: query.data.map((m) => ({
          buyer_demand_id: m.buyer_demand_id,
          village_id: m.village.id,
          buyer_name: m.buyer_name,
          crop_id: m.crop_id,
          crop_name: names.get(m.crop_id) || m.crop_id,
          window_start: m.window_start,
          window_end: m.window_end,
          demand_kg: num(m.demand_kg),
          available_kg: num(m.available_kg),
          committed_kg: num(m.committed_kg),
          coverage_pct: num(m.coverage_pct),
          opportunity_id: m.opportunity?.id ?? null,
          opportunity_status: m.opportunity?.status ?? null,
        })),
      },
    [query.data, names],
  )

  return { ...query, data }
}

export function useTowerQuality(villageId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.tower.quality(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => (await farmTowerDataQuality(workspaceId(), villageId!)).data,
  })
}

/** A village's live crop cycles with their current expected and actual figures (display join). */
async function cyclesWithFigures(villageId: string) {
  const workspace = workspaceId()
  const [cycles, reports] = await Promise.all([
    allPages((p) => farmRecordsCropCyclesList(workspace, { village_id: villageId, ...p })),
    allPages((p) => farmRecordsHarvestReportsAll(workspace, { village_id: villageId, current: true, ...p })),
  ])
  const figure = (cycleId: string, kind: string) =>
    reports.find((r) => r.crop_cycle_id === cycleId && r.kind === kind && r.is_current) ?? null
  return cycles.map((c) => ({ cycle: c, expected: figure(c.id, 'expected'), actual: figure(c.id, 'actual') }))
}

export interface QualityRecord {
  id: string
  label: string
  detail: string
  qualifies: boolean
  kind: 'person' | 'farm' | 'cycle'
}

/** The records behind one data-quality ratio; the ratio itself is the API's. */
export function useTowerQualityRows(villageId: string | undefined, metric: QualityMetric) {
  const language = useLanguage()
  return useQuery({
    queryKey: [...queryKeys.tower.quality(villageId ?? ''), 'rows', metric, language],
    enabled: Boolean(villageId),
    queryFn: async (): Promise<QualityRecord[]> => {
      const workspace = workspaceId()
      const village = { village_id: villageId! }
      if (metric === 'persons') {
        const rows = await allPages((p) => farmRecordsPersonsList(workspace, { ...village, ...p }))
        return rows.map((row) => ({
          id: row.id,
          label: `${row.given_name} ${row.family_name}`,
          detail: row.provenance.verification,
          qualifies: row.provenance.verification === 'verified',
          kind: 'person',
        }))
      }
      if (metric === 'farms') {
        const rows = await allPages((p) => farmRecordsFarmsList(workspace, { ...village, ...p }))
        return rows.map((row) => ({
          id: row.id,
          label: row.label,
          detail: row.latitude === null ? '' : `${row.latitude}, ${row.longitude ?? '—'}`,
          qualifies: row.latitude !== null,
          kind: 'farm',
        }))
      }
      return (await cyclesWithFigures(villageId!)).map(({ cycle, expected }) => ({
        id: cycle.id,
        label: localisedName(cycle.crop, language) || cycle.season_label || cycle.id,
        detail: cycle.season_label ?? '',
        qualifies: expected !== null,
        kind: 'cycle',
      }))
    },
  })
}

/** The requests behind the energy figures, for the drill-down. */
export function useTowerEnergyRows(villageId: string | undefined) {
  const language = useLanguage()
  const query = useQuery({
    queryKey: [...queryKeys.tower.energy(villageId ?? ''), 'rows'],
    enabled: Boolean(villageId),
    queryFn: () => allPages((p) => farmPueRequestsList(workspaceId(), { village_id: villageId!, ...p })),
  })

  const data = useMemo(
    () =>
      query.data?.map((row) => ({
        id: row.id,
        status: row.status as Enums['pue_status'],
        applicant: row.person.name,
        person_id: row.person.id,
        equipment_name: localisedName(row.equipment, language),
        est_power_kw: num(row.estimate?.est_power_kw),
      })),
    [query.data, language],
  )

  return { ...query, data }
}

/**
 * The crop cycles behind the production figures, each reachable on the officer surface (spec 8.2).
 * Plot label and farmer (the farm's primary manager) are joined from the village's lists, display only.
 */
export function useTowerProductionRows(villageId: string | undefined) {
  const language = useLanguage()
  const query = useQuery({
    queryKey: [...queryKeys.tower.production(villageId ?? ''), 'cycles'],
    enabled: Boolean(villageId),
    queryFn: async () => {
      const workspace = workspaceId()
      const village = { village_id: villageId! }
      const [cycles, plots, farms, persons] = await Promise.all([
        cyclesWithFigures(villageId!),
        allPages((p) => farmRecordsPlotsList(workspace, { ...village, ...p })),
        allPages((p) => farmRecordsFarmsList(workspace, { ...village, ...p })),
        allPages((p) => farmRecordsPersonsList(workspace, { ...village, ...p })),
      ])
      return cycles.map(({ cycle, expected, actual }) => {
        const plot = plots.find((p) => p.id === cycle.plot_id)
        const farm = farms.find((f) => f.id === plot?.farm_id)
        const manager = farm?.managers.find((m) => m.is_primary) ?? farm?.managers[0]
        const person = persons.find((p) => p.id === manager?.person_id)
        return {
          id: cycle.id,
          crop: cycle.crop,
          season_label: cycle.season_label,
          area_ha: num(cycle.area_ha),
          harvest_start: cycle.harvest_start,
          harvest_end: cycle.harvest_end,
          verification: cycle.provenance.verification as Enums['verification_status'],
          plot_label: plot?.label ?? null,
          farmer: person ? `${person.given_name} ${person.family_name}` : null,
          person_id: person?.id ?? null,
          expected_kg: num(expected?.quantity_kg),
          actual_kg: num(actual?.quantity_kg),
        }
      })
    },
  })

  const data = useMemo(
    () => query.data?.map((row) => ({ ...row, crop_name: localisedName(row.crop, language) })),
    [query.data, language],
  )

  return { ...query, data }
}
