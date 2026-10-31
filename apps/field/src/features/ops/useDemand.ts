import { useMemo } from 'react'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { recoverInsert } from '@/lib/recoverInsert'
import { EarlierVersionSavedError } from '@/lib/drafts'
import { isUuid } from '@/lib/ids'
import { localisedName } from '@/lib/names'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'

type DemandRow = Database['public']['Tables']['buyer_demand']['Row']
type MatchRow = Database['public']['Views']['v_demand_match']['Row']

const DEMAND_SELECT = `id, project_id, buyer_id, crop_id, quantity_kg, quality_note,
  window_start, window_end, delivery_point, indicative_price_per_kg, currency, status,
  captured_at, captured_by, verification, confidence,
  buyer ( id, name, channel ),
  crop ( id, name_en, name_sw )`

export interface Demand extends DemandRow {
  buyer_name: string
  crop_name: string
}

function shape(row: Record<string, unknown>, sw: boolean): Demand {
  const buyer = row.buyer as { name: string } | null
  const crop = row.crop as { name_en: string; name_sw: string } | null
  return {
    ...(row as unknown as DemandRow),
    buyer_name: buyer?.name ?? '',
    crop_name: crop ? (sw ? crop.name_sw : crop.name_en) : '',
  }
}

export function useDemands() {
  const { i18n } = useTranslation()
  const sw = i18n.resolvedLanguage === 'sw'

  const query = useQuery({
    queryKey: queryKeys.demands('all'),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('buyer_demand')
        .select(DEMAND_SELECT)
        .is('deleted_at', null)
        .order('window_start')
      if (error) throw new Error(error.message)
      return (data ?? []) as unknown as Array<Record<string, unknown>>
    },
  })

  return { ...query, demands: (query.data ?? []).map((r) => shape(r, sw)) }
}

export function useDemand(demandId: string) {
  const { i18n } = useTranslation()
  const sw = i18n.resolvedLanguage === 'sw'

  const query = useQuery({
    queryKey: queryKeys.demand(demandId),
    queryFn: async () => {
      // QA #3: a malformed route param must reach the same "not found" state
      // as a well-formed id matching nothing, not a uuid parse failure.
      if (!isUuid(demandId)) return null

      const { data, error } = await supabase
        .from('buyer_demand')
        .select(DEMAND_SELECT)
        .eq('id', demandId)
        .maybeSingle()
      if (error) throw new Error(error.message)
      return (data as unknown as Record<string, unknown>) ?? null
    },
  })

  return { ...query, demand: query.data ? shape(query.data, sw) : null }
}

/**
 * v_demand_match — the comparison, not a match.
 *
 * Overlapping windows only, no allocation and no ranking. A demand with no
 * overlapping supply produces NO ROWS, which is why the screen has to state
 * the zero rather than infer it from an empty table.
 */
export function useDemandMatches(demandId: string) {
  return useQuery({
    queryKey: [...queryKeys.demand(demandId), 'matches'],
    queryFn: async () => {
      const { data, error } = await supabase
        .from('v_demand_match')
        .select('*')
        .eq('buyer_demand_id', demandId)
      if (error) throw new Error(error.message)
      return (data ?? []) as MatchRow[]
    },
  })
}

/** Buyers and crops, for the create form. */
export function useDemandFormOptions() {
  const { i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  const query = useQuery({
    queryKey: ['demand-form-options'],
    staleTime: 5 * 60_000,
    queryFn: async () => {
      const [buyers, crops] = await Promise.all([
        supabase.from('buyer').select('id, name, project_id').eq('is_active', true).order('name'),
        supabase.from('crop').select('id, name_en, name_sw').eq('is_active', true).order('name_en'),
      ])
      if (buyers.error) throw new Error(buyers.error.message)
      if (crops.error) throw new Error(crops.error.message)

      return {
        buyers: buyers.data ?? [],
        // Both names; the language is chosen at render — QA #31.
        crops: (crops.data ?? []).map((c) => ({
          id: c.id,
          name_en: c.name_en,
          name_sw: c.name_sw,
        })),
      }
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

export interface NewDemand {
  id?: string
  projectId: string
  buyerId: string
  cropId: string
  quantityKg: number
  windowStart: string
  windowEnd: string
  deliveryPoint: string
  pricePerKg: string
  qualityNote: string
}

/**
 * buyer_demand carries NO source column: the five categories classify how a
 * fact about the productive economy was learned, and a buyer's stated
 * requirement is a counterparty input. captured_by is set instead.
 *
 * demand_window_sane is not pre-checked here. The constraint is called and its
 * message surfaced — a client-side copy would drift.
 */
export function useCreateDemand() {
  const queryClient = useQueryClient()

  // One list for success and for a recovered earlier save, so they cannot drift.
  const invalidate = () => queryClient.invalidateQueries({ queryKey: queryKeys.demands('all') })

  return useMutation({
    mutationFn: async (input: NewDemand) => {
      const submitted = {
        project_id: input.projectId,
        buyer_id: input.buyerId,
        crop_id: input.cropId,
        quantity_kg: input.quantityKg,
        window_start: input.windowStart,
        window_end: input.windowEnd,
        delivery_point: input.deliveryPoint || null,
        indicative_price_per_kg: input.pricePerKg === '' ? null : Number(input.pricePerKg),
        quality_note: input.qualityNote || null,
      }
      const { data, error } = await supabase
        .from('buyer_demand')
        .insert({ id: input.id, ...submitted })
        .select('id')
        .single()

      if (error) return recoverInsert('buyer_demand', input.id, error, submitted)
      return data
    },
    onSuccess: invalidate,
    // The earlier submission saved: the list the user is sent to must hold it.
    onError: async (error) => {
      if (error instanceof EarlierVersionSavedError) await invalidate()
    },
  })
}

/** Creates an opportunity for one village against one demand. */
export function useCreateOpportunity(demandId: string) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (input: { villageId: string; cropId: string; note?: string }) => {
      const { data, error } = await supabase
        .from('opportunity')
        .insert({
          buyer_demand_id: demandId,
          village_id: input.villageId,
          crop_id: input.cropId,
          note: input.note ?? null,
          // offered_quantity_kg is left alone: opportunity_resum maintains it
          // from the supply lines, so the client never sets it.
        })
        .select('id')
        .single()

      if (error) throw new Error(error.message)
      return data
    },
    onSuccess: async (_data, input) => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.demand(demandId) })
      await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(input.villageId) })
    },
  })
}

/**
 * v_village_supply for one crop — expected, committed and available per
 * village and window, all summed by the view.
 *
 * v_demand_match exposes available_kg and the coverage it calculated, but not
 * the committed slice, and the committed slice is what makes "available" mean
 * anything. This reads the view's own committed_kg rather than deriving it:
 * the client does not aggregate (business-rules §11).
 */
export function useVillageSupply(cropId: string | undefined) {
  return useQuery({
    queryKey: ['village-supply', cropId],
    enabled: Boolean(cropId),
    queryFn: async () => {
      const { data, error } = await supabase
        .from('v_village_supply')
        .select('village_id, window_month, expected_kg, committed_kg, available_kg')
        .eq('crop_id', cropId!)
      if (error) throw new Error(error.message)
      return data ?? []
    },
  })
}
