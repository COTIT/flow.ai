import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { localisedName, type LocalisedNames } from '@/lib/names'
import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

type Enums = Database['public']['Enums']

/** What the query caches: both names per crop, neither chosen yet. */
export interface FarmerOpportunityRow {
  id: string
  status: Enums['opportunity_status']
  /** From this farmer's OWN supply lines — see the note on the query. */
  crops: LocalisedNames[]
  /** The whole opportunity, re-summed by opportunity_resum from all its lines. */
  offered_quantity_kg: number | null
  /** This farmer's own share of it. */
  my_contribution_kg: number
}

/**
 * Opportunities this farmer's supply is inside — spec 6.6.
 *
 * No filter by farmer: `opportunity_read_contributor` scopes to
 * `app_supplied_opportunities()`, which resolves the opportunities whose
 * supply lines trace back to `app_farms()`. Asking for "mine" is the same
 * query as asking for "opportunities", and adding a client-side filter would
 * imply the client is what keeps other farmers' rows out.
 *
 * The embedded `opportunity_supply` rows are likewise already scoped, so
 * summing them gives this farmer's contribution and not the opportunity's
 * total — which is read from `offered_quantity_kg`, the column
 * `opportunity_resum` maintains.
 *
 * **`buyer_demand` is deliberately NOT read here.** Its policy is
 * `demand_read ... using (app_is_staff() ...)`, so a farmer cannot see it at
 * all: no buyer name, no demand window, no demand crop. Asking anyway returns
 * nulls and renders a screen full of blanks, which is what the first version
 * of this hook did. The crop is taken instead from the farmer's OWN supply
 * lines, through `crop_cycle` — rows `app_farms()` already grants her.
 */
export async function fetchFarmerOpportunities(): Promise<FarmerOpportunityRow[]> {
  const { data, error } = await supabase
    .from('opportunity')
    .select(
      `id, status, offered_quantity_kg,
       opportunity_supply ( contributed_kg, crop_cycle ( crop ( name_en, name_sw ) ) )`,
    )
    .is('deleted_at', null)
    .order('created_at', { ascending: false })

  if (error) throw new Error(error.message)

  return (data ?? []).map((row) => {
    const raw = row as unknown as Record<string, unknown>
    const lines = (raw.opportunity_supply ?? []) as Array<{
      contributed_kg: number
      crop_cycle?: { crop?: { name_en: string; name_sw: string } | null } | null
    }>

    // The crops across this farmer's own lines, both names kept. Normally one:
    // an opportunity is per demand and a demand names a single crop.
    const crops = lines.map((l) => l.crop_cycle?.crop).filter(Boolean) as LocalisedNames[]

    return {
      id: raw.id as string,
      status: raw.status as Enums['opportunity_status'],
      crops,
      offered_quantity_kg: (raw.offered_quantity_kg as number) ?? null,
      // Summing rows RLS already scoped to this farmer. Not a village
      // aggregate — business-rules §11 keeps those in views, and there is no
      // view of "one farmer's share of an opportunity".
      my_contribution_kg: lines.reduce((sum, l) => sum + Number(l.contributed_kg), 0),
    }
  })
}

/** A cached row with its crop names resolved for the active language. */
export type FarmerOpportunity = FarmerOpportunityRow & { crop_name: string }

export function useFarmerOpportunities() {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  const query = useQuery({
    queryKey: queryKeys.farmerOpportunities(),
    queryFn: fetchFarmerOpportunities,
  })

  // Named at render, so one cache entry serves both languages and a switch
  // shows immediately — QA #31.
  const data = useMemo(
    () =>
      query.data?.map((row) => ({
        ...row,
        crop_name: [...new Set(row.crops.map((c) => localisedName(c, language)))]
          .filter(Boolean)
          .join(' · '),
      })),
    [query.data, language],
  )

  return { ...query, data }
}
