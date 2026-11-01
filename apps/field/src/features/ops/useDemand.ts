import { useMemo, useRef } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  farmMarketBuyersList,
  farmMarketDemandMatches,
  farmMarketDemandsCreate,
  farmMarketDemandsGet,
  farmMarketDemandsList,
  farmMarketOpportunitiesCreate,
} from '@genesy/api-client/endpoints/farm-market'
import type { BuyerDemandRequest, BuyerDemandResource, Money } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { fetchCrops } from '@/features/officer/useCrops'
import { apiStatus } from '@/lib/apiError'
import type { Enums } from '@/lib/enums'
import { isUuid, newUuid } from '@/lib/ids'
import { localisedName } from '@/lib/names'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'

export interface Demand {
  id: string
  buyer_name: string
  crop_name: string
  /** kg as a number for display only; the API keeps the exact figure. */
  quantity_kg: number
  window_start: string
  window_end: string
  indicative_price_per_kg: Money | null
  delivery_point: string | null
  quality_note: string | null
  status: Enums['demand_status']
}

function shape(row: BuyerDemandResource, language: string | undefined): Demand {
  return {
    id: row.id,
    buyer_name: row.buyer.name,
    crop_name: localisedName(row.crop, language),
    quantity_kg: Number(row.quantity_kg),
    window_start: row.window_start,
    window_end: row.window_end,
    indicative_price_per_kg: row.indicative_price_per_kg,
    delivery_point: row.delivery_point,
    quality_note: row.quality_note,
    status: row.status as Demand['status'],
  }
}

/** The order book, by window start (the API's order). Ops and admin only. */
export function useDemands() {
  const { i18n } = useTranslation()

  const query = useQuery({
    queryKey: queryKeys.demands('all'),
    queryFn: () => {
      const workspace = workspaceId()
      return allPages((params) => farmMarketDemandsList(workspace, params))
    },
  })

  return { ...query, demands: (query.data ?? []).map((r) => shape(r, i18n.resolvedLanguage)) }
}

export function useDemand(demandId: string) {
  const { i18n } = useTranslation()

  const query = useQuery({
    queryKey: queryKeys.demand(demandId),
    queryFn: async () => {
      // QA #3: a malformed route param must reach the same "not found" state
      // as a well-formed id matching nothing.
      if (!isUuid(demandId)) return null
      try {
        return (await farmMarketDemandsGet(workspaceId(), demandId)).data
      } catch (error) {
        if (apiStatus(error) === 404) return null
        throw error
      }
    },
  })

  return { ...query, demand: query.data ? shape(query.data, i18n.resolvedLanguage) : null }
}

export interface DemandMatch {
  village_id: string
  village_name: string
  available_kg: number
  committed_kg: number
  coverable_kg: number
  coverage_pct: number
  opportunity_id: string | null
  opportunity_status: string | null
}

/**
 * The API's comparison of this demand with each village's expected harvest — not a match: nothing
 * is ranked or assigned. No overlapping supply is NO ROWS, which the screen states. Ops and admin
 * only (others get 403, shown as written). Figures are the API's, as numbers for display only.
 */
export function useDemandMatches(demandId: string) {
  return useQuery({
    queryKey: [...queryKeys.demand(demandId), 'matches'],
    queryFn: async (): Promise<DemandMatch[]> => {
      const rows = (await farmMarketDemandMatches(workspaceId(), demandId)).data
      return rows.map((m) => ({
        village_id: m.village.id,
        village_name: m.village.name,
        available_kg: Number(m.available_kg),
        committed_kg: Number(m.committed_kg),
        coverable_kg: Number(m.coverable_kg),
        coverage_pct: Number(m.coverage_pct),
        opportunity_id: m.opportunity?.id ?? null,
        opportunity_status: m.opportunity?.status ?? null,
      }))
    },
  })
}

/** Active buyers and crops, for the create form. */
export function useDemandFormOptions() {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  const query = useQuery({
    queryKey: ['demand-form-options'],
    staleTime: 5 * 60_000,
    queryFn: async () => {
      const workspace = workspaceId()
      const [buyers, crops] = await Promise.all([
        allPages((params) => farmMarketBuyersList(workspace, { active: true, ...params })),
        fetchCrops(),
      ])
      // Both names kept; the language is chosen at render — QA #31.
      return { buyers: buyers.map(({ id, name }) => ({ id, name })), crops }
    },
  })

  const data = useMemo(
    () =>
      query.data
        ? {
            buyers: query.data.buyers,
            crops: query.data.crops.map((c) => ({ id: c.id, name: localisedName(c, language) })),
          }
        : query.data,
    [query.data, language],
  )

  return { ...query, data }
}

/**
 * Creates a demand from the body the form built (the draft's id, kg as decimal text, price in
 * integer minor units). The window, price and currency rules are the API's; its message is shown.
 */
export function useCreateDemand() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (body: BuyerDemandRequest) => (await farmMarketDemandsCreate(workspaceId(), body)).data,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: queryKeys.demands('all') }),
  })
}

/**
 * Creates an opportunity for one village against this demand; the API gives it the demand's crop.
 * Each village's id is kept until that create succeeds, so a retry after a lost reply replays the
 * same opportunity instead of being refused as a second one.
 */
export function useCreateOpportunity(demandId: string) {
  const queryClient = useQueryClient()
  const ids = useRef(new Map<string, string>())

  return useMutation({
    mutationFn: async ({ villageId }: { villageId: string }) => {
      const id = ids.current.get(villageId) ?? newUuid()
      ids.current.set(villageId, id)
      return (await farmMarketOpportunitiesCreate(workspaceId(), demandId, { id, village_id: villageId })).data
    },
    onSuccess: async (_data, { villageId }) => {
      ids.current.delete(villageId)
      await queryClient.invalidateQueries({ queryKey: queryKeys.demand(demandId) })
      await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId) })
    },
  })
}
