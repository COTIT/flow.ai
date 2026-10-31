import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { queryKeys } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'

type Provenance = {
  source: Database['public']['Enums']['source_type']
  verification: Database['public']['Enums']['verification_status']
  confidence: Database['public']['Enums']['confidence_level'] | null
  captured_at: string
}

export interface MyFarm extends Provenance {
  id: string
  label: string
  latitude: number | null
  longitude: number | null
  plots: Array<
    Provenance & {
      id: string
      label: string
      area_ha: number | null
      cycles: Array<
        Provenance & {
          id: string
          crop_name: string
          season_label: string | null
          area_ha: number | null
          tree_count: number | null
          unit_count: number | null
          harvest_start: string | null
          harvest_end: string | null
          expected: (Provenance & { id: string; quantity_kg: number }) | null
        }
      >
    }
  >
}

/**
 * The farmer's own farms — spec 6.2.
 *
 * No filter by farm id is needed or wanted: farm_read_own scopes to
 * app_farms(), which resolves the farms this person manages or whose household
 * owns them. Asking for "my farms" is the same query as asking for "farms".
 *
 * Only CURRENT expected figures are shown. harvest_one_current guarantees at
 * most one per cycle per kind, which is what keeps a superseded estimate from
 * appearing beside the figure that replaced it.
 *
 * Embeds name their foreign key: village_id is denormalised down the chain and
 * held true by composite FKs, so each pair has two relationships and PostgREST
 * refuses to guess between them.
 */
export async function fetchMyFarms(): Promise<MyFarm[]> {
  const { data, error } = await supabase
    .from('farm')
    .select(
      `id, label, latitude, longitude, source, verification, confidence, captured_at,
       plot!plot_farm_id_fkey ( id, label, area_ha, source, verification, confidence, captured_at,
         crop_cycle!crop_cycle_plot_id_fkey ( id, season_label, area_ha, tree_count, unit_count,
           harvest_start, harvest_end, source, verification, confidence, captured_at,
           crop ( name_en, name_sw ),
           harvest_report!harvest_report_crop_cycle_id_fkey ( id, quantity_kg, kind, is_current,
             source, verification, confidence, captured_at )
         )
       )`,
    )
    .is('deleted_at', null)
    .order('label')

  if (error) throw new Error(error.message)
  return (data ?? []) as unknown as MyFarm[]
}

export function useMyFarm() {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  const query = useQuery({
    queryKey: queryKeys.farms('mine'),
    queryFn: fetchMyFarms,
  })

  const farms = (query.data ?? []).map((farm) => {
    const rawPlots = (farm as unknown as { plot?: unknown[] }).plot ?? farm.plots ?? []
    return {
      ...farm,
      plots: (rawPlots as Array<Record<string, unknown>>).map((plot) => {
        const rawCycles = (plot.crop_cycle ?? []) as Array<Record<string, unknown>>
        return {
          ...(plot as unknown as MyFarm['plots'][number]),
          cycles: rawCycles.map((cycle) => {
            const crop = cycle.crop as { name_en: string; name_sw: string } | null
            const reports = (cycle.harvest_report ?? []) as Array<{
              id: string
              quantity_kg: number
              kind: string
              is_current: boolean
              source: Provenance['source']
              verification: Provenance['verification']
              confidence: Provenance['confidence']
              captured_at: string
            }>
            // Current expected only. A superseded estimate must never appear
            // beside the figure that replaced it.
            const expected = reports.find((r) => r.kind === 'expected' && r.is_current) ?? null
            return {
              ...(cycle as unknown as MyFarm['plots'][number]['cycles'][number]),
              crop_name: crop ? (language === 'sw' ? crop.name_sw : crop.name_en) : '',
              expected,
            }
          }),
        }
      }),
    }
  }) as MyFarm[]

  return { ...query, farms }
}
