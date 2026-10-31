import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { isUuid } from '@/lib/ids'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'
import { recoverInsert } from '@/lib/recoverInsert'
import { EarlierVersionSavedError } from '@/lib/drafts'

type OpportunityRow = Database['public']['Tables']['opportunity']['Row']
type AvailableRow = Database['public']['Views']['v_harvest_available']['Row']

export interface SupplyLine {
  harvest_report_id: string
  crop_cycle_id: string
  contributed_kg: number
  farmer: string | null
  person_id: string | null
  plot_label: string | null
  crop_name: string
  /** Who attached the line; null for seeded lines written with no actor. */
  captured_by: string | null
  captured_at: string | null
}

export interface OpportunityDetail extends OpportunityRow {
  buyer_name: string
  crop_name: string
  village_name: string
  demand_quantity_kg: number | null
  supply: SupplyLine[]
}

/**
 * The opportunity with its supply lines.
 *
 * opportunity_supply -> harvest_report -> crop_cycle -> plot -> farm is the
 * product's traceability claim, enforced structurally by those foreign keys
 * rather than reported. Embeds name their FK because village_id is held true
 * by composite keys down that chain.
 */
export function useOpportunity(opportunityId: string) {
  const { i18n } = useTranslation()
  const sw = i18n.resolvedLanguage === 'sw'

  const query = useQuery({
    queryKey: queryKeys.opportunity(opportunityId),
    queryFn: async () => {
      // QA #3: a malformed route param must reach the same "not found" state
      // as a well-formed id matching nothing, not a uuid parse failure.
      if (!isUuid(opportunityId)) return null

      const { data, error } = await supabase
        .from('opportunity')
        .select(
          `id, buyer_demand_id, village_id, crop_id, offered_quantity_kg, status, note,
           captured_at, captured_by,
           village ( id, name ),
           crop ( id, name_en, name_sw ),
           buyer_demand ( id, quantity_kg, buyer ( id, name ) ),
           opportunity_supply (
             harvest_report_id, crop_cycle_id, contributed_kg, captured_by, captured_at,
             crop_cycle!opportunity_supply_crop_cycle_id_fkey (
               id,
               crop ( name_en, name_sw ),
               plot!crop_cycle_plot_id_fkey (
                 id, label,
                 farm!plot_farm_id_fkey (
                   id,
                   farm_manager ( person ( id, given_name, family_name ) )
                 )
               )
             )
           )`,
        )
        .eq('id', opportunityId)
        .maybeSingle()

      if (error) throw new Error(error.message)
      return (data as unknown as Record<string, unknown>) ?? null
    },
  })

  const detail: OpportunityDetail | null = query.data
    ? (() => {
        const raw = query.data
        const village = raw.village as { name: string } | null
        const crop = raw.crop as { name_en: string; name_sw: string } | null
        const demand = raw.buyer_demand as
          | { quantity_kg: number; buyer: { name: string } | null }
          | null
        const lines = (raw.opportunity_supply ?? []) as Array<Record<string, unknown>>

        return {
          ...(raw as unknown as OpportunityRow),
          buyer_name: demand?.buyer?.name ?? '',
          crop_name: crop ? (sw ? crop.name_sw : crop.name_en) : '',
          village_name: village?.name ?? '',
          demand_quantity_kg: demand?.quantity_kg ?? null,
          supply: lines.map((line) => {
            const cycle = line.crop_cycle as Record<string, unknown> | null
            const cycleCrop = cycle?.crop as { name_en: string; name_sw: string } | null
            const plot = cycle?.plot as Record<string, unknown> | null
            const farmer = farmerName(plot?.farm as FarmWithManagers | null)

            return {
              harvest_report_id: line.harvest_report_id as string,
              crop_cycle_id: line.crop_cycle_id as string,
              contributed_kg: line.contributed_kg as number,
              farmer: farmer.name,
              person_id: farmer.personId,
              plot_label: (plot?.label as string) ?? null,
              crop_name: cycleCrop ? (sw ? cycleCrop.name_sw : cycleCrop.name_en) : '',
              captured_by: (line.captured_by as string | null) ?? null,
              captured_at: (line.captured_at as string | null) ?? null,
            }
          }),
        }
      })()
    : null

  return { ...query, opportunity: detail }
}

interface FarmWithManagers {
  farm_manager?: Array<{
    person: { id: string; given_name: string; family_name: string } | null
  }> | null
}

/**
 * The farmer a farm is shown under: its first manager. One rule for the
 * supply lines and the harvest picker, so the name chosen before Attach is the
 * name listed after it.
 */
export function farmerName(farm: FarmWithManagers | null | undefined): {
  name: string | null
  personId: string | null
} {
  const person = farm?.farm_manager?.[0]?.person ?? null
  return {
    name: person ? `${person.given_name} ${person.family_name}` : null,
    personId: person?.id ?? null,
  }
}

interface PlotWithFarm {
  id: string
  label: string | null
  farm: FarmWithManagers | null
}

export interface AvailableHarvest extends AvailableRow {
  farmer: string | null
  plot_label: string | null
}

/**
 * Names each available harvest by its plot and farmer, joined on `plot_id`.
 * A plot RLS did not return leaves the harvest listed without names: zero rows
 * is an answer, and the figure is still the guard's to refuse.
 */
export function withGrowers(rows: AvailableRow[], plots: PlotWithFarm[]): AvailableHarvest[] {
  const byId = new Map(plots.map((p) => [p.id, p]))
  return rows.map((row) => {
    const plot = row.plot_id ? byId.get(row.plot_id) : undefined
    return {
      ...row,
      farmer: farmerName(plot?.farm).name,
      plot_label: plot?.label ?? null,
    }
  })
}

/**
 * v_harvest_available for this village and crop — current expected figures
 * with their committed and available quantities already worked out — named by
 * farmer and plot. Picking a figure is picking a farmer, and Attach cannot be
 * undone, so the picker says whose harvest it is before the choice.
 *
 * Rows with nothing left are still listed: the guard is what refuses an
 * over-commitment, and hiding them would be a client-side pre-check.
 */
export function useAvailableHarvest(villageId: string | undefined, cropId: string | undefined) {
  return useQuery({
    queryKey: ['harvest-available', villageId, cropId],
    enabled: Boolean(villageId && cropId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('v_harvest_available')
        .select('*')
        .eq('village_id', villageId!)
        .eq('crop_id', cropId!)
      if (error) throw new Error(error.message)
      const rows = (data ?? []) as AvailableRow[]

      const plotIds = [...new Set(rows.map((r) => r.plot_id).filter((id): id is string => Boolean(id)))]
      if (plotIds.length === 0) return withGrowers(rows, [])

      // The same plot -> farm -> farm_manager -> person path the supply lines
      // embed, under the same policies.
      const { data: plots, error: plotError } = await supabase
        .from('plot')
        .select(
          `id, label,
           farm!plot_farm_id_fkey (
             id,
             farm_manager ( person ( id, given_name, family_name ) )
           )`,
        )
        .in('id', plotIds)
      if (plotError) throw new Error(plotError.message)

      return withGrowers(rows, (plots ?? []) as unknown as PlotWithFarm[])
    },
  })
}

/**
 * Attaches supply.
 *
 * opportunity_supply_guard raises the over-commitment message and it is shown
 * as written. There is deliberately no client-side pre-check: a copy of the
 * guard would drift, and the message names the actual numbers.
 */
export function useAttachSupply(opportunityId: string, villageId: string | undefined) {
  const queryClient = useQueryClient()

  // One list for success and for a recovered earlier save, so they cannot drift.
  const invalidate = async () => {
    await queryClient.invalidateQueries({ queryKey: queryKeys.opportunity(opportunityId) })
    await queryClient.invalidateQueries({ queryKey: ['harvest-available'] })
    if (villageId) {
      await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId) })
    }
  }

  return useMutation({
    mutationFn: async (input: {
      id?: string
      harvestReportId: string
      cropCycleId: string
      contributedKg: number
    }) => {
      const submitted = {
        opportunity_id: opportunityId,
        harvest_report_id: input.harvestReportId,
        crop_cycle_id: input.cropCycleId,
        contributed_kg: input.contributedKg,
      }
      const { error } = await supabase.from('opportunity_supply').insert({ id: input.id, ...submitted })
      if (error) await recoverInsert('opportunity_supply', input.id, error, submitted)
    },
    onSuccess: invalidate,
    // The earlier submission saved: the list the user is sent to must hold it.
    onError: async (error) => {
      if (error instanceof EarlierVersionSavedError) await invalidate()
    },
  })
}

/**
 * Moves an opportunity along its status machine — business-rules §7 and §8.
 *
 * Only `status` is sent. Everything else on the row belongs to whoever wrote
 * it, and `updated_at` is the `opportunity_updated_at` trigger's to stamp.
 *
 * **Declining or lapsing is how supply is released.** `v_harvest_available`
 * counts `opportunity_supply` on opportunities in `proposed`/`shared`/
 * `accepted` only, so leaving that set returns every committed kg to
 * `available_kg`. There is no detach and no delete — `opportunity_supply` has
 * neither a DELETE policy nor a `deleted_at`, by design, because the supply
 * line is the traceability record and erasing it would erase the history of
 * what was offered.
 *
 * That is why the invalidation list is wider than the row that changed: the
 * available figures, the demand's coverage and the market tile all read
 * through that same filter and all move on this one write.
 */
export function useOpportunityStatus(
  opportunityId: string,
  villageId: string | undefined,
  demandId: string | undefined,
) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (status: Database['public']['Enums']['opportunity_status']) => {
      const { error } = await supabase.from('opportunity').update({ status }).eq('id', opportunityId)
      if (error) throw new Error(error.message)
    },
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
