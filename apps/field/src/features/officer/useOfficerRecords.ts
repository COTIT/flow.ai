import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { queryKeys } from '@/lib/queryKeys'
import { isUuid } from '@/lib/ids'
import { localisedName, type LocalisedNames } from '@/lib/names'
import { supabase } from '@/lib/supabase'
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

const PROVENANCE = 'source, verification, confidence, captured_at'

/**
 * Farm detail — spec 5.5, with section-level officer corrections.
 *
 * Edits are handled by the shared officer edit RPC; this module stays focused
 * on the provenance graph read used by the detail screen.
 *
 * The plot embed names its foreign key. village_id is denormalised down
 * farm → plot and held true by a composite FK, so the pair has two
 * relationships and PostgREST refuses to choose between them.
 */
export async function fetchFarmDetail(farmId: string): Promise<FarmDetail | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(farmId)) return null

  const { data, error } = await supabase
    .from('farm')
    .select(
      `id, label, village_id, latitude, longitude, ${PROVENANCE},
       plot!plot_farm_id_fkey ( id, label, area_ha, latitude, longitude, ${PROVENANCE} )`,
    )
    .eq('id', farmId)
    .is('deleted_at', null)
    .maybeSingle()

  if (error) throw new Error(error.message)
  if (!data) return null

  const raw = data as unknown as Record<string, unknown>
  return {
    ...(raw as unknown as FarmDetail),
    plots: (raw.plot ?? []) as FarmDetail['plots'],
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
 * a revision goes through `app_supersede_harvest`; old rows remain visible.
 */
export async function fetchCycleDetail(cycleId: string): Promise<CycleRow | null> {
  // QA #3: a malformed route param must reach the same "not found" state as a
  // well-formed id matching nothing, rather than a uuid parse failure.
  if (!isUuid(cycleId)) return null

  const { data, error } = await supabase
    .from('crop_cycle')
    .select(
      `id, village_id, crop_id, season_label, status, area_ha, tree_count, unit_count,
       planted_on, harvest_start, harvest_end, ${PROVENANCE},
       crop ( name_en, name_sw ),
       plot!crop_cycle_plot_id_fkey ( label ),
       harvest_report!harvest_report_crop_cycle_id_fkey (
         id, kind, quantity_kg, reported_for, is_current, ${PROVENANCE} )`,
    )
    .eq('id', cycleId)
    .is('deleted_at', null)
    .maybeSingle()

  if (error) throw new Error(error.message)
  if (!data) return null

  const raw = data as unknown as Record<string, unknown>
  const crop = raw.crop as { name_en: string; name_sw: string } | null
  const plot = raw.plot as { label: string } | null
  const harvests = ((raw.harvest_report ?? []) as CycleHarvest[])
    // Current first, then most recently reported — the figure in force leads,
    // and the ones it replaced follow in the order they were superseded.
    .slice()
    .sort((a, b) => {
      if (a.is_current !== b.is_current) return a.is_current ? -1 : 1
      return (b.reported_for ?? '').localeCompare(a.reported_for ?? '')
    })

  return {
    ...(raw as unknown as CycleRow),
    // Both names, so the cache holds both and the language is chosen at
    // render — QA #31.
    crop,
    plot_label: plot?.label ?? null,
    harvests,
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
