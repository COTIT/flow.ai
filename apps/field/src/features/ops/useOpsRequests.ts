import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { supabase } from '@/lib/supabase'
import { isUuid } from '@/lib/ids'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'
import type { RequestSearch } from '@/features/ops/requestSearch'
import { ACTION_TARGET, type RequestStatus, type ReviewerAction } from '@/features/ops/transitions'
import type { Database } from '@/lib/db.types'

type RequestRow = Database['public']['Tables']['pue_request']['Row']
type EstimateRow = Database['public']['Tables']['energy_estimate']['Row']

const SELECT = `id, village_id, person_id, farm_id, equipment_id, acquisition, quantity, hours_per_day,
  days_per_week, purpose, status, submitted_at, decided_at, decision_note,
  source, verification, confidence, captured_at, captured_by,
  person ( id, given_name, family_name ),
  village ( id, name ),
  farm ( id, label ),
  equipment ( id, name_en, name_sw, rated_power_kw, indicative_price, currency ),
  energy_estimate ( id, rated_power_kw, quantity, hours_per_day, days_per_week,
    est_power_kw, est_kwh_per_day, est_kwh_per_week, method, computed_at )`

export interface OpsRequest extends RequestRow {
  applicant: string
  village_name: string
  farm_label: string | null
  equipment_name: string
  estimate: EstimateRow | null
}

function shape(row: Record<string, unknown>, sw: boolean): OpsRequest {
  const person = row.person as { given_name: string; family_name: string } | null
  const village = row.village as { name: string } | null
  const farm = row.farm as { label: string } | null
  const equipment = row.equipment as { name_en: string; name_sw: string } | null
  const estimates = row.energy_estimate as EstimateRow[] | EstimateRow | null

  return {
    ...(row as unknown as RequestRow),
    applicant: person ? `${person.given_name} ${person.family_name}` : '',
    village_name: village?.name ?? '',
    farm_label: farm?.label ?? null,
    equipment_name: equipment ? (sw ? equipment.name_sw : equipment.name_en) : '',
    estimate: Array.isArray(estimates) ? (estimates[0] ?? null) : (estimates ?? null),
  }
}

/**
 * The pipeline — spec 7.2. Filters come from the URL already validated, so
 * they are applied to the query rather than to the rendered rows: filtering in
 * the client would ship rows the user then cannot see, and would make the
 * count in an empty state a lie.
 */
export function useOpsRequests(search: RequestSearch) {
  const { i18n } = useTranslation()
  const sw = i18n.resolvedLanguage === 'sw'

  const query = useQuery({
    queryKey: queryKeys.requests({ villageId: search.village, status: search.status }),
    queryFn: async () => {
      let builder = supabase.from('pue_request').select(SELECT).is('deleted_at', null)
      if (search.status) builder = builder.eq('status', search.status)
      if (search.village) builder = builder.eq('village_id', search.village)

      const { data, error } = await builder.order('created_at', { ascending: false })
      if (error) throw new Error(error.message)
      return (data ?? []) as unknown as Array<Record<string, unknown>>
    },
  })

  return { ...query, requests: (query.data ?? []).map((r) => shape(r, sw)) }
}

export function useOpsRequest(requestId: string) {
  const { i18n } = useTranslation()
  const sw = i18n.resolvedLanguage === 'sw'

  const query = useQuery({
    queryKey: queryKeys.request(requestId),
    queryFn: async () => {
      // QA #3: a malformed route param must reach the same "not found" state
      // as a well-formed id matching nothing, not a uuid parse failure.
      if (!isUuid(requestId)) return null

      const { data, error } = await supabase
        .from('pue_request')
        .select(SELECT)
        .eq('id', requestId)
        .maybeSingle()
      if (error) throw new Error(error.message)
      return (data as unknown as Record<string, unknown>) ?? null
    },
  })

  return { ...query, request: query.data ? shape(query.data, sw) : null }
}

/** v_village_energy — planned capacity, prospective and approved peaks. */
export function useVillageEnergy(villageId: string | undefined) {
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
      return data
    },
  })
}

/**
 * Moves a request through the status machine.
 *
 * Sends only `status` and, for a decision, `decision_note`. submitted_at,
 * decided_at and decided_by are stamped by pue_request_guard and the client
 * must never send them — if it does, the trigger overwrites them anyway, and
 * asserting who decided is exactly what the trigger exists to prevent.
 */
export function useReviewAction(requestId: string, villageId: string | undefined) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: async ({ action, note }: { action: ReviewerAction; note?: string }) => {
      const patch: { status: RequestStatus; decision_note?: string } = {
        status: ACTION_TARGET[action],
      }
      if (note) patch.decision_note = note

      const { error } = await supabase.from('pue_request').update(patch).eq('id', requestId)
      // Guard messages are written to be read by humans. An "illegal
      // transition" reaching a user means this UI offered a control it
      // should not have.
      if (error) throw new Error(error.message)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.request(requestId) })
      await queryClient.invalidateQueries({ queryKey: ['requests'] })
      if (villageId) {
        await queryClient.invalidateQueries({ queryKey: queryKeys.tower.energy(villageId) })
        await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId) })
      }
    },
  })
}
