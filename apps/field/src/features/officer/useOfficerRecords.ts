import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  farmRecordsCropCyclesGet,
  farmRecordsFarmsGet,
  farmRecordsHarvestReportsList,
  farmRecordsPlotsGet,
  farmRecordsPlotsList,
} from '@genesy/api-client/endpoints/farm-records'
import type { Provenance as ApiProvenance } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { apiStatus } from '@/lib/apiError'
import { queryKeys } from '@/lib/queryKeys'
import { isUuid } from '@/lib/ids'
import { localisedName, type LocalisedNames } from '@/lib/names'
import type { Database } from '@/lib/db.types'

type Enums = Database['public']['Enums']

interface Provenance {
  source: Enums['source_type']
  verification: Enums['verification_status']
  confidence: Enums['confidence_level'] | null
  captured_at: string
}

export interface FarmDetail extends Provenance {
  id: string
  label: string
  village_id: string
  latitude: number | null
  longitude: number | null
  plots: Array<Provenance & {
    id: string
    label: string
    area_ha: number | null
    latitude: number | null
    longitude: number | null
  }>
}

export interface CycleHarvest extends Provenance {
  id: string
  kind: Enums['harvest_kind']
  quantity_kg: number
  reported_for: string | null
  is_current: boolean
}

/** What the query caches: both names, no language chosen yet. */
export interface CycleRow extends Provenance {
  id: string
  crop_id: string
  village_id: string
  /** Both columns, so the language can be chosen at render — QA #31. */
  crop: LocalisedNames | null
  season_label: string | null
  status: Enums['crop_cycle_status']
  area_ha: number | null
  tree_count: number | null
  unit_count: number | null
  planted_on: string | null
  harvest_start: string | null
  harvest_end: string | null
  plot_label: string | null
  harvests: CycleHarvest[]
}

/** Decimal text from the API as a number, for display only (GPS, hectares). */
const shown = (value: string | null) => (value === null ? null : Number(value))

/**
 * Farm detail — spec 5.5, with section-level officer corrections:
 * `GET …/farms/{farm}` and `GET …/plots?farm_id=` (every page).
 */
export async function fetchFarmDetail(farmId: string): Promise<FarmDetail | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(farmId)) return null

  const workspace = workspaceId()
  let farm
  try {
    farm = (await farmRecordsFarmsGet(workspace, farmId)).data
  } catch (error) {
    // Another village's farm and a missing one are the same answer: not found.
    if (apiStatus(error) === 404) return null
    throw error
  }
  const plots = await allPages((params) => farmRecordsPlotsList(workspace, { farm_id: farmId, ...params }))

  return {
    id: farm.id,
    label: farm.label,
    village_id: farm.village_id,
    latitude: shown(farm.latitude),
    longitude: shown(farm.longitude),
    ...provenanceOf(farm.provenance),
    plots: plots.map((plot) => ({
      id: plot.id,
      label: plot.label,
      area_ha: shown(plot.area_ha),
      latitude: shown(plot.latitude),
      longitude: shown(plot.longitude),
      ...provenanceOf(plot.provenance),
    })),
  }
}

export function useFarmDetail(farmId: string) {
  return useQuery({
    queryKey: queryKeys.farm(farmId),
    queryFn: () => fetchFarmDetail(farmId),
  })
}

/**
 * Crop cycle detail with its harvest series — spec 5.6.
 *
 * A harvest figure is a SERIES, not a value (business-rules §6): superseded
 * rows stay auditable and are shown, labelled, beside the current one. Adding
 * a revision goes through `POST /farm/crop-cycles/{cycle}/harvest-reports`; old rows remain visible.
 */
export async function fetchCycleDetail(cycleId: string): Promise<CycleRow | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(cycleId)) return null

  const workspace = workspaceId()
  let cycle
  try {
    cycle = (await farmRecordsCropCyclesGet(workspace, cycleId)).data
  } catch (error) {
    // Another village's cycle and a missing one are the same answer: not found.
    if (apiStatus(error) === 404) return null
    throw error
  }
  const [plot, series] = await Promise.all([
    farmRecordsPlotsGet(workspace, cycle.plot_id),
    allPages((params) => farmRecordsHarvestReportsList(workspace, cycleId, params)),
  ])

  const harvests = series
    .map((h) => ({
      ...provenanceOf(h.provenance),
      id: h.id,
      kind: h.kind as Enums['harvest_kind'],
      // Display only: a weight, never money, and never summed here.
      quantity_kg: Number(h.quantity_kg),
      reported_for: h.reported_for,
      is_current: h.is_current,
    }))
    // Current first, then most recently reported — the figure in force leads,
    // and the ones it replaced follow in the order they were superseded.
    .sort((a, b) => {
      if (a.is_current !== b.is_current) return a.is_current ? -1 : 1
      return (b.reported_for ?? '').localeCompare(a.reported_for ?? '')
    })

  return {
    ...provenanceOf(cycle.provenance),
    id: cycle.id,
    crop_id: cycle.crop_id,
    village_id: cycle.village_id,
    // Both names, so the cache holds both and the language is chosen at
    // render — QA #31.
    crop: { name_en: cycle.crop.name_en, name_sw: cycle.crop.name_sw },
    season_label: cycle.season_label,
    status: cycle.status as Enums['crop_cycle_status'],
    area_ha: cycle.area_ha === null ? null : Number(cycle.area_ha),
    tree_count: cycle.tree_count,
    unit_count: cycle.unit_count,
    planted_on: cycle.planted_on,
    harvest_start: cycle.harvest_start,
    harvest_end: cycle.harvest_end,
    plot_label: plot.data.label,
    harvests,
  }
}

export function provenanceOf(p: ApiProvenance): Provenance {
  return {
    source: p.source as Provenance['source'],
    verification: p.verification as Provenance['verification'],
    confidence: p.confidence as Provenance['confidence'],
    captured_at: p.captured_at,
  }
}

/** A cached cycle with its name resolved for the active language. */
export type CycleDetail = CycleRow & { crop_name: string }

export function useCycleDetail(cycleId: string) {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  const query = useQuery({
    queryKey: queryKeys.cycle(cycleId),
    queryFn: () => fetchCycleDetail(cycleId),
  })

  // The crop name is chosen HERE, not in the queryFn: the key is about the
  // cycle, so a name chosen at fetch time would be served in whichever locale
  // resolved first, for good (QA #31).
  const cycle = useMemo(
    () =>
      query.data
        ? { ...query.data, crop_name: localisedName(query.data.crop, language) }
        : null,
    [query.data, language],
  )

  return { ...query, cycle }
}
