import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { localisedName, type LocalisedNames } from '@/lib/names'
import { queryKeys } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'
import type { QualityMetric } from '@/features/tower/towerSearch'

type Enums = Database['public']['Enums']

/**
 * The Tower reads views and nothing else.
 *
 * All aggregation lives in SQL (business-rules §7): the client never sums,
 * never derives coverage, and never combines prospective with approved. If a
 * figure is not in a view, it does not belong on this screen.
 *
 * Crop names are looked up separately rather than embedded: the views declare
 * no foreign-key relationships, so PostgREST cannot embed through them.
 */
/**
 * Both names per crop — QA #31.
 *
 * The choice between them belongs at render: these rows are cached under keys
 * about the VILLAGE, so a name chosen here would be served in whichever locale
 * resolved first, for good.
 */
/** The active language, for the render-time name selection below. */
function useLanguage(): string | undefined {
  return useTranslation().i18n.resolvedLanguage
}

async function cropNames(): Promise<Map<string, LocalisedNames>> {
  const { data, error } = await supabase.from('crop').select('id, name_en, name_sw')
  if (error) throw new Error(error.message)
  return new Map((data ?? []).map((c) => [c.id, { name_en: c.name_en, name_sw: c.name_sw }]))
}

export function useTowerProduction(villageId: string | undefined) {
  const language = useLanguage()

  const query = useQuery({
    queryKey: queryKeys.tower.production(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => {
      const [rows, names] = await Promise.all([
        supabase.from('v_village_production').select('*').eq('village_id', villageId!),
        cropNames(),
      ])
      if (rows.error) throw new Error(rows.error.message)
      return (rows.data ?? []).map((r) => ({ ...r, crop: names.get(r.crop_id ?? '') ?? null }))
    },
  })

  const data = useMemo(
    () =>
      query.data?.map((r) => ({
        ...r,
        crop_name: localisedName(r.crop, language) || (r.crop_id ?? ''),
      })),
    [query.data, language],
  )

  return { ...query, data }
}

export function useTowerPipeline(villageId: string | undefined) {
  return useQuery({
    queryKey: [...queryKeys.tower.market(villageId ?? ''), 'pue'],
    enabled: Boolean(villageId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('v_village_pue_pipeline')
        .select('*')
        .eq('village_id', villageId!)
      if (error) throw new Error(error.message)
      return data ?? []
    },
  })
}

export function useTowerEnergy(villageId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.tower.energy(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('v_village_energy')
        .select('*')
        .eq('village_id', villageId!)
        .maybeSingle()
      if (error) throw new Error(error.message)
      // A village with no CURRENT capacity row does not appear in this view at
      // all, because it inner-joins village_capacity. That is a legitimate
      // empty answer rather than an error.
      return data
    },
  })
}

export function useTowerMarket(villageId: string | undefined) {
  const language = useLanguage()

  const query = useQuery({
    queryKey: queryKeys.tower.market(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => {
      const [supply, matches, names] = await Promise.all([
        supabase.from('v_village_supply').select('*').eq('village_id', villageId!),
        supabase.from('v_demand_match').select('*').eq('village_id', villageId!),
        cropNames(),
      ])
      if (supply.error) throw new Error(supply.error.message)
      if (matches.error) throw new Error(matches.error.message)

      const demandIds = [
        ...new Set((matches.data ?? []).map((m) => m.buyer_demand_id).filter(Boolean)),
      ] as string[]

      const buyers = demandIds.length
        ? await supabase
            .from('buyer_demand')
            .select('id, buyer ( name )')
            .in('id', demandIds)
        : { data: [], error: null }
      if (buyers.error) throw new Error(buyers.error.message)

      const buyerByDemand = new Map(
        (buyers.data ?? []).map((d) => {
          const buyer = (d as unknown as { buyer?: { name: string } | null }).buyer
          return [d.id, buyer?.name ?? '']
        }),
      )

      return {
        supply: (supply.data ?? []).map((s) => ({ ...s, crop: names.get(s.crop_id ?? '') ?? null })),
        matches: (matches.data ?? []).map((m) => ({
          ...m,
          crop: names.get(m.crop_id ?? '') ?? null,
          buyer_name: buyerByDemand.get(m.buyer_demand_id ?? '') ?? '',
        })),
      }
    },
  })

  const data = useMemo(() => {
    if (!query.data) return query.data
    const named = <T extends { crop: LocalisedNames | null; crop_id: string | null }>(row: T) => ({
      ...row,
      crop_name: localisedName(row.crop, language) || (row.crop_id ?? ''),
    })
    return {
      supply: query.data.supply.map(named),
      matches: query.data.matches.map(named),
    }
  }, [query.data, language])

  return { ...query, data }
}

export function useTowerQuality(villageId: string | undefined) {
  return useQuery({
    queryKey: queryKeys.tower.quality(villageId ?? ''),
    enabled: Boolean(villageId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('v_village_data_quality')
        .select('*')
        .eq('village_id', villageId!)
        .maybeSingle()
      if (error) throw new Error(error.message)
      return data
    },
  })
}

export interface QualityRecord {
  id: string
  label: string
  detail: string
  qualifies: boolean
  kind: 'person' | 'farm' | 'cycle'
}

/** Uses the same village, soft-delete and numerator predicates as the SQL view. */
export function useTowerQualityRows(villageId: string | undefined, metric: QualityMetric) {
  const language = useLanguage()
  const query = useQuery({
    queryKey: [...queryKeys.tower.quality(villageId ?? ''), 'rows', metric, language],
    enabled: Boolean(villageId),
    queryFn: async () => {
      if (metric === 'persons') {
        const { data, error } = await supabase
          .from('person')
          .select('id, given_name, family_name, verification')
          .eq('village_id', villageId!)
          .is('deleted_at', null)
          .order('given_name')
        if (error) throw new Error(error.message)
        return (data ?? []).map((row) => ({
          id: row.id,
          label: `${row.given_name} ${row.family_name}`,
          detail: row.verification,
          qualifies: row.verification === 'verified',
          kind: 'person' as const,
        }))
      }
      if (metric === 'farms') {
        const { data, error } = await supabase
          .from('farm')
          .select('id, label, latitude, longitude')
          .eq('village_id', villageId!)
          .is('deleted_at', null)
          .order('label')
        if (error) throw new Error(error.message)
        return (data ?? []).map((row) => ({
          id: row.id,
          label: row.label,
          detail: row.latitude === null ? '' : `${row.latitude}, ${row.longitude ?? '—'}`,
          qualifies: row.latitude !== null,
          kind: 'farm' as const,
        }))
      }
      const { data, error } = await supabase
        .from('crop_cycle')
        .select(`id, season_label, crop (name_en, name_sw),
          harvest_report!harvest_report_crop_cycle_id_fkey (id, kind, is_current, deleted_at)`)
        .eq('village_id', villageId!)
        .is('deleted_at', null)
        .order('harvest_start')
      if (error) throw new Error(error.message)
      return (data ?? []).map((row) => {
        const raw = row as unknown as {
          id: string
          season_label: string | null
          crop: LocalisedNames | null
          harvest_report: Array<{
            kind: string
            is_current: boolean
            deleted_at: string | null
          }>
        }
        return {
          id: raw.id,
          label: localisedName(raw.crop, language) || raw.season_label || raw.id,
          detail: raw.season_label ?? '',
          qualifies: raw.harvest_report.some(
            (report) => report.kind === 'expected' && report.is_current && report.deleted_at === null,
          ),
          kind: 'cycle' as const,
        }
      })
    },
  })
  return query
}

/** The requests behind the energy figures, for the drill-down. */
export function useTowerEnergyRows(villageId: string | undefined) {
  const language = useLanguage()

  const query = useQuery({
    queryKey: [...queryKeys.tower.energy(villageId ?? ''), 'rows'],
    enabled: Boolean(villageId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('pue_request')
        .select(
          `id, status, quantity,
           person ( id, given_name, family_name ),
           equipment ( name_en, name_sw ),
           energy_estimate ( est_power_kw, est_kwh_per_week )`,
        )
        .eq('village_id', villageId!)
        .is('deleted_at', null)
        .order('status')

      if (error) throw new Error(error.message)

      return (data ?? []).map((row) => {
        const raw = row as unknown as Record<string, unknown>
        const person = raw.person as { id: string; given_name: string; family_name: string } | null
        const equipment = raw.equipment as { name_en: string; name_sw: string } | null
        const estimates = raw.energy_estimate as
          | Array<{ est_power_kw: number | null }>
          | { est_power_kw: number | null }
          | null
        const estimate = Array.isArray(estimates) ? (estimates[0] ?? null) : estimates

        return {
          id: raw.id as string,
          status: raw.status as Enums['pue_status'],
          applicant: person ? `${person.given_name} ${person.family_name}` : '',
          person_id: person?.id ?? null,
          equipment,
          est_power_kw: estimate?.est_power_kw ?? null,
        }
      })
    },
  })

  const data = useMemo(
    () =>
      query.data?.map((row) => ({
        ...row,
        equipment_name: localisedName(row.equipment, language),
      })),
    [query.data, language],
  )

  return { ...query, data }
}

/**
 * The crop cycles behind the production figures.
 *
 * v_village_production is grouped by crop and window, so it has no single
 * cycle to point at. Spec 8.2 requires the drill-down to end in a link to an
 * actual row, so the rows here are the cycles themselves — each one reachable
 * on the officer surface.
 */
export function useTowerProductionRows(villageId: string | undefined) {
  const language = useLanguage()

  const query = useQuery({
    queryKey: [...queryKeys.tower.production(villageId ?? ''), 'cycles'],
    enabled: Boolean(villageId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('crop_cycle')
        .select(
          `id, season_label, area_ha, tree_count, unit_count, harvest_start, harvest_end,
           status, verification,
           crop ( name_en, name_sw ),
           plot!crop_cycle_plot_id_fkey (
             id, label,
             farm!plot_farm_id_fkey (
               id, label,
               farm_manager ( person ( id, given_name, family_name ) )
             )
           ),
           harvest_report!harvest_report_crop_cycle_id_fkey ( kind, quantity_kg, is_current )`,
        )
        .eq('village_id', villageId!)
        .is('deleted_at', null)
        .order('harvest_start')

      if (error) throw new Error(error.message)

      return (data ?? []).map((row) => {
        const raw = row as unknown as Record<string, unknown>
        const crop = raw.crop as { name_en: string; name_sw: string } | null
        const plot = raw.plot as Record<string, unknown> | null
        const farm = plot?.farm as Record<string, unknown> | null
        const managers = (farm?.farm_manager ?? []) as Array<{
          person: { id: string; given_name: string; family_name: string } | null
        }>
        const person = managers[0]?.person ?? null
        const reports = (raw.harvest_report ?? []) as Array<{
          kind: string
          quantity_kg: number
          is_current: boolean
        }>

        // Current figures only. A superseded estimate never reaches a total.
        const expected = reports.find((r) => r.kind === 'expected' && r.is_current) ?? null
        const actual = reports.find((r) => r.kind === 'actual' && r.is_current) ?? null

        return {
          id: raw.id as string,
          crop,
          season_label: (raw.season_label as string) ?? null,
          area_ha: (raw.area_ha as number) ?? null,
          harvest_start: (raw.harvest_start as string) ?? null,
          harvest_end: (raw.harvest_end as string) ?? null,
          verification: raw.verification as Enums['verification_status'],
          plot_label: (plot?.label as string) ?? null,
          farmer: person ? `${person.given_name} ${person.family_name}` : null,
          person_id: person?.id ?? null,
          expected_kg: expected?.quantity_kg ?? null,
          actual_kg: actual?.quantity_kg ?? null,
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
