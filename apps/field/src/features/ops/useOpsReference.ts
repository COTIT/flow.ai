import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { farmVillagesCapacityCreate, farmVillagesCreate, farmVillagesList } from '@genesy/api-client/endpoints/farm-villages'
import type { CapacityRequest } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
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
  /** Decimal text as the API sends it (3 decimals). */
  simultaneity_factor: string | null
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
 * Villages with their CURRENT capacity — spec 7.9.
 *
 * The API embeds the current capacity on each village, or `null` when there is none (or the caller
 * is neither ops/admin nor staff of that village). Absent capacity is not zero capacity, so its
 * figures stay absent.
 */
export async function fetchVillageCapacity(): Promise<VillageCapacity[]> {
  const workspace = workspaceId()
  const rows = await allPages((params) => farmVillagesList(workspace, params))
  return rows.map((row) => ({
    id: row.id,
    name: row.name,
    code: row.code,
    // kW for display only; the API keeps the exact figure.
    capacity_kw: row.capacity ? Number(row.capacity.capacity_kw) : null,
    basis: (row.capacity?.basis ?? null) as VillageCapacity['basis'],
    simultaneity_factor: row.capacity?.simultaneity_factor ?? null,
    effective_from: row.capacity?.effective_from ?? null,
    source_note: row.capacity?.source_note ?? null,
  }))
}

export function useVillageCapacity() {
  return useQuery({ queryKey: queryKeys.villageCapacity(), queryFn: fetchVillageCapacity })
}

/** A capacity as typed: decimal text, blanks allowed where the API has a default. */
export interface CapacityInput {
  capacity_kw: string
  basis: string
  simultaneity_factor: string
  source_note: string
  effective_from: string
}

export interface NewVillage {
  id: string
  project_id: string
  name: string
  code: string
  latitude: string
  longitude: string
  capacity: CapacityInput
}

const orNull = (text: string) => (text.trim() === '' ? null : text.trim())

/** Typed text as the API's capacity body: figures stay decimal text, never floats. */
const capacityBody = (c: CapacityInput): CapacityRequest => ({
  capacity_kw: c.capacity_kw.trim(),
  basis: orNull(c.basis),
  simultaneity_factor: orNull(c.simultaneity_factor),
  source_note: orNull(c.source_note),
  effective_from: orNull(c.effective_from),
})

function useInvalidateVillages() {
  const queryClient = useQueryClient()
  return async () => {
    await queryClient.invalidateQueries({ queryKey: queryKeys.villageCapacity() })
    // The scope names, the Tower's village picker and its tiles all read villages.
    await queryClient.invalidateQueries({ queryKey: queryKeys.villages() })
    await queryClient.invalidateQueries({ queryKey: ['tower'] })
  }
}

/**
 * Create a village with its first capacity — one call, one transaction on the API. The id is the
 * draft's client ref, so a retried submit replays (200) rather than adding a second village.
 * Range, code and basis rules are the API's; its message is shown as written.
 */
export function useCreateVillage() {
  const invalidate = useInvalidateVillages()
  return useMutation({
    mutationFn: async (village: NewVillage) =>
      (
        await farmVillagesCreate(workspaceId(), {
          id: village.id,
          name: village.name,
          code: village.code,
          latitude: orNull(village.latitude),
          longitude: orNull(village.longitude),
          capacity: capacityBody(village.capacity),
        })
      ).data,
    onSuccess: invalidate,
  })
}

/**
 * Record a new current capacity for a village; the previous one stays as history on the API.
 * shortcut: the capacity create takes no client id, so a retry after a lost reply records the same
 * figure twice (harmless: the newest is current); send one when the API accepts it.
 */
export function useSetVillageCapacity() {
  const invalidate = useInvalidateVillages()
  return useMutation({
    mutationFn: async ({ villageId, capacity }: { villageId: string; capacity: CapacityInput }) =>
      (await farmVillagesCapacityCreate(workspaceId(), villageId, capacityBody(capacity))).data,
    onSuccess: invalidate,
  })
}
