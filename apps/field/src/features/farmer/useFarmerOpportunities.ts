import { useMemo } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import { farmMarketOpportunitiesList } from '@genesy/api-client/endpoints/farm-market'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { localisedName, type LocalisedNames } from '@/lib/names'
import { queryKeys } from '@/lib/queryKeys'
import type { Enums } from '@/lib/enums'


/** What the query caches: both names per crop, neither chosen yet. */
export interface FarmerOpportunityRow {
  id: string
  status: Enums['opportunity_status']
  crops: LocalisedNames[]
  /** The whole opportunity, summed by the API from all its lines. */
  offered_quantity_kg: number | null
  /** This farmer's own share, worked out by the API; null when the account has no person record. */
  my_contribution_kg: number | null
}

/**
 * Opportunities this farmer's supply is inside — spec 6.6.
 *
 * The API scopes the list (a farmer sees an opportunity only when their own harvest is in it) and
 * sends `my_contribution_kg` from their own lines, so nothing is filtered or added up here.
 * Buyer and demand details are not shown to a farmer: they stay with ops.
 */
export async function fetchFarmerOpportunities(): Promise<FarmerOpportunityRow[]> {
  const workspace = workspaceId()
  const rows = await allPages((params) => farmMarketOpportunitiesList(workspace, params))
  // kg as numbers for display only; the API keeps the exact figures.
  const kg = (text: string | null) => (text === null ? null : Number(text))
  return rows.map((row) => ({
    id: row.id,
    status: row.status as Enums['opportunity_status'],
    crops: [row.crop],
    offered_quantity_kg: kg(row.offered_quantity_kg),
    my_contribution_kg: kg(row.my_contribution_kg),
  }))
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
