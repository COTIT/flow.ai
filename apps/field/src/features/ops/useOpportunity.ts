import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'
import {
  farmMarketHarvestAvailable,
  farmMarketOpportunitiesGet,
  farmMarketOpportunitiesStatus,
  farmMarketSupplyCreate,
} from '@genesy/api-client/endpoints/farm-market'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import type { OpportunityStatus } from '@/features/ops/opportunityTransitions'
import { apiStatus } from '@/lib/apiError'
import { isUuid } from '@/lib/ids'
import { localisedName } from '@/lib/names'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'

/** kg as a number for display only; the API keeps the exact figure. */
const kg = (text: string) => Number(text)

export interface SupplyLine {
  harvest_report_id: string
  crop_cycle_id: string
  contributed_kg: number
  farmer: string | null
  person_id: string | null
  plot_label: string | null
  crop_name: string
  /** Who attached the line, by name; null when the API names nobody. */
  captured_by: string | null
  captured_at: string | null
}

export interface OpportunityDetail {
  id: string
  buyer_demand_id: string
  village_id: string
  crop_id: string
  status: OpportunityStatus
  offered_quantity_kg: number
  buyer_name: string
  crop_name: string
  village_name: string
  demand_quantity_kg: number
  supply: SupplyLine[]
}

/**
 * The opportunity with its supply lines, each naming its farmer, plot and attacher as the API
 * embeds them. Every line drills to the cycle and farmer under it: the traceability claim.
 */
export function useOpportunity(opportunityId: string) {
  const { i18n } = useTranslation()

  const query = useQuery({
    queryKey: queryKeys.opportunity(opportunityId),
    queryFn: async () => {
      // QA #3: a malformed route param must reach the same "not found" state
      // as a well-formed id matching nothing.
      if (!isUuid(opportunityId)) return null
      try {
        return (await farmMarketOpportunitiesGet(workspaceId(), opportunityId)).data
      } catch (error) {
        if (apiStatus(error) === 404) return null
        throw error
      }
    },
  })

  const raw = query.data
  const cropName = raw ? localisedName(raw.crop, i18n.resolvedLanguage) : ''
  const detail: OpportunityDetail | null = raw
    ? {
        id: raw.id,
        buyer_demand_id: raw.demand.id,
        village_id: raw.village.id,
        crop_id: raw.crop.id,
        status: raw.status as OpportunityStatus,
        offered_quantity_kg: kg(raw.offered_quantity_kg),
        buyer_name: raw.demand.buyer.name,
        crop_name: cropName,
        village_name: raw.village.name,
        demand_quantity_kg: kg(raw.demand.quantity_kg),
        supply: raw.supply.map((line) => ({
          harvest_report_id: line.harvest_report_id,
          crop_cycle_id: line.crop_cycle_id,
          contributed_kg: kg(line.contributed_kg),
          farmer: line.farmer?.name ?? null,
          person_id: line.farmer?.person_id ?? null,
          plot_label: line.plot_label,
          // A line must match the opportunity's crop (the API refuses any other).
          crop_name: cropName,
          captured_by: line.captured_by?.name ?? null,
          captured_at: line.captured_at,
        })),
      }
    : null

  return { ...query, opportunity: detail }
}

export interface AvailableHarvest {
  harvest_report_id: string
  farmer: string | null
  plot_label: string
  quantity_kg: number
  available_kg: number
  harvest_start: string | null
}

/**
 * Current expected harvests for this village and crop with the API's committed and available
 * figures, named by farmer and plot. Picking a figure is picking a farmer, and Attach cannot be
 * undone, so the picker says whose harvest it is before the choice.
 *
 * Rows with nothing left are still listed: the API is what refuses an over-commitment.
 */
export function useAvailableHarvest(villageId: string | undefined, cropId: string | undefined) {
  return useQuery({
    queryKey: ['harvest-available', villageId, cropId],
    enabled: Boolean(villageId && cropId),
    queryFn: async (): Promise<AvailableHarvest[]> => {
      const workspace = workspaceId()
      const rows = await allPages((params) =>
        farmMarketHarvestAvailable(workspace, { village_id: villageId, crop_id: cropId, ...params }),
      )
      return rows.map((r) => ({
        harvest_report_id: r.harvest_report_id,
        farmer: r.farmer?.name ?? null,
        plot_label: r.plot_label,
        quantity_kg: kg(r.quantity_kg),
        available_kg: kg(r.available_kg),
        harvest_start: r.harvest_start,
      }))
    },
  })
}

/**
 * Attaches supply. `id` is the draft's client ref, kept until success, so a retry after a lost
 * reply replays the same line (and an edited resend is refused) instead of committing twice.
 * The kg go as the decimal text typed. Over-commitment is the API's to refuse; its message names
 * the actual kilograms and is shown as written.
 */
export function useAttachSupply(opportunityId: string, villageId: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (input: { id: string; harvestReportId: string; contributedKg: string }) =>
      (
        await farmMarketSupplyCreate(workspaceId(), opportunityId, {
          id: input.id,
          harvest_report_id: input.harvestReportId,
          contributed_kg: input.contributedKg,
        })
      ).data,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.opportunity(opportunityId) })
      await queryClient.invalidateQueries({ queryKey: ['harvest-available'] })
      if (villageId) {
        await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId) })
      }
    },
  })
}

/**
 * Moves an opportunity along its status machine; the API enforces the transitions and refuses
 * any other with its own message.
 *
 * **Declining or lapsing is how supply is released**: only live opportunities hold kg. There is
 * no detach: supply lines are never edited or removed, they are the record of what was offered.
 * That is why the invalidation list is wider than the row that changed.
 */
export function useOpportunityStatus(
  opportunityId: string,
  villageId: string | undefined,
  demandId: string | undefined,
) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (status: OpportunityStatus) =>
      (await farmMarketOpportunitiesStatus(workspaceId(), opportunityId, { status })).data,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.opportunity(opportunityId) })
      await queryClient.invalidateQueries({ queryKey: ['harvest-available'] })
      if (demandId) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.demand(demandId) })
      }
      // A farmer's own view of this opportunity, and the list it sits in.
      await queryClient.invalidateQueries({ queryKey: ['farmerOpportunities'] })
      if (villageId) {
        await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId) })
      }
    },
  })
}
