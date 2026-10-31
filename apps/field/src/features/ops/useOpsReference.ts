import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'
import { recoverInsert } from '@/lib/recoverInsert'
import { EarlierVersionSavedError } from '@/lib/drafts'

type Enums = Database['public']['Enums']

export interface Buyer {
  id: string
  project_id: string
  name: string
  channel: Enums['buyer_channel']
  contact_note: string | null
  is_active: boolean
}

export interface VillageCapacity {
  id: string
  name: string
  code: string
  capacity_kw: number | null
  basis: Enums['capacity_basis'] | null
  simultaneity_factor: number | null
  effective_from: string | null
  source_note: string | null
}

export const BUYER_CHANNELS: Enums['buyer_channel'][] = ['direct', 'afm', 'other']

/**
 * Buyers — spec 7.5. Scoped by `buyer_read` to `app_projects()`.
 *
 * `buyer` is a CONFIG table: business-rules §4 lists it among the tables with
 * no provenance columns, so nothing here goes through `withProvenance`. A
 * buyer's stated requirement is a counterparty input, not a fact learned about
 * the productive economy.
 */
export async function fetchBuyers(): Promise<Buyer[]> {
  const { data, error } = await supabase
    .from('buyer')
    .select('id, project_id, name, channel, contact_note, is_active')
    .order('name')

  if (error) throw new Error(error.message)
  return (data ?? []) as Buyer[]
}

export function useBuyers() {
  return useQuery({ queryKey: queryKeys.buyers(), queryFn: fetchBuyers })
}

export interface NewBuyer {
  id?: string
  project_id: string
  name: string
  channel: Enums['buyer_channel']
  contact_note: string | null
}

/**
 * Create a buyer.
 *
 * `buyer_write` requires `app_manages_project`, so RLS decides whether this
 * caller may. The unique (project_id, name) constraint is left to the
 * database: its error is surfaced rather than pre-checked, per
 * business-rules §0.3.
 */
export function useCreateBuyer() {
  const queryClient = useQueryClient()

  // One list for success and for a recovered earlier save, so they cannot drift.
  const invalidate = async () => {
    // The demand form's buyer picker reads the same rows.
    await queryClient.invalidateQueries({ queryKey: queryKeys.buyers() })
    await queryClient.invalidateQueries({ queryKey: ['demands'] })
  }

  return useMutation({
    mutationFn: async (buyer: NewBuyer) => {
      const { error } = await supabase.from('buyer').insert(buyer)
      if (error) await recoverInsert('buyer', buyer.id, error, { ...buyer })
    },
    onSuccess: invalidate,
    // The earlier submission saved: the list the user is sent to must hold it.
    onError: async (error) => {
      if (error instanceof EarlierVersionSavedError) await invalidate()
    },
  })
}

/**
 * Villages with their CURRENT capacity row — spec 7.9.
 *
 * `village_capacity_one_current` permits one current row per village, so the
 * embed returns at most one. A village with no capacity row still appears,
 * with its figures absent rather than zeroed: absent capacity is not zero
 * capacity, and `v_village_energy` inner-joins capacity so such a village has
 * no energy figures at all.
 */
export async function fetchVillageCapacity(): Promise<VillageCapacity[]> {
  const { data, error } = await supabase
    .from('village')
    .select(
      'id, name, code, village_capacity ( capacity_kw, basis, simultaneity_factor, effective_from, source_note, is_current )',
    )
    .order('name')

  if (error) throw new Error(error.message)

  return (data ?? []).map((row) => {
    const raw = row as unknown as Record<string, unknown>
    const rows = (raw.village_capacity ?? []) as Array<Record<string, unknown>>
    const current = rows.find((c) => c.is_current) ?? null

    return {
      id: raw.id as string,
      name: raw.name as string,
      code: raw.code as string,
      capacity_kw: (current?.capacity_kw ?? null) as number | null,
      basis: (current?.basis ?? null) as VillageCapacity['basis'],
      simultaneity_factor: (current?.simultaneity_factor ?? null) as number | null,
      effective_from: (current?.effective_from ?? null) as string | null,
      source_note: (current?.source_note ?? null) as string | null,
    }
  })
}

export function useVillageCapacity() {
  return useQuery({ queryKey: queryKeys.villageCapacity(), queryFn: fetchVillageCapacity })
}

export interface NewVillage {
  id: string
  project_id: string
  name: string
  code: string
  latitude: string
  longitude: string
  capacity: {
    capacity_kw: string
    basis: Enums['capacity_basis']
    simultaneity_factor: string
    source_note: string
    effective_from: string
  }
}

/**
 * Create a village with its first capacity row — one RPC, one transaction.
 *
 * `app_create_village` is SECURITY INVOKER: `village_write` requires
 * `app_manages_project`, so RLS decides. The id is the draft's client ref, so
 * a retried submit replays rather than adding a second village. Range, code
 * and basis rules are the database's; its message is shown as written.
 */
export function useCreateVillage() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async (village: NewVillage) => {
      const { data, error } = await supabase.rpc('app_create_village', { payload: { ...village } })
      if (error) throw new Error(error.message)
      return data as { village_id: string; replayed: boolean }
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.villageCapacity() })
      // The scope names, the Tower's village picker and its tiles all read villages.
      await queryClient.invalidateQueries({ queryKey: queryKeys.villages() })
      await queryClient.invalidateQueries({ queryKey: ['tower'] })
    },
  })
}
