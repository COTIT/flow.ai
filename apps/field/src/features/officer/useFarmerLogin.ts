import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { queryKeys } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Database } from '@/lib/db.types'

/**
 * What `app_farmer_login_issue` hands back: the ONLY place the temporary
 * password exists in plain text. It lives in the mutation result and in the
 * component showing it — never a query, never a draft, never a URL, never a
 * log line.
 */
export interface FarmerLoginIssued {
  phone: string
  temp_password: string
  kind: 'initial' | 'reset'
}

/** One row of `app_person_login_history`. Names and dates, never a password. */
export type LoginHistoryRow =
  Database['public']['Functions']['app_person_login_history']['Returns'][number]

/**
 * Create a person's app login, or reset its password if one exists — the
 * database decides which, and records every call. The staff-village scope, the
 * phone requirement and the staff-account refusal are all its rules, so none
 * of them is checked here.
 */
export async function issueFarmerLogin(personId: string): Promise<FarmerLoginIssued> {
  const { data, error } = await supabase.rpc('app_farmer_login_issue', { p_person_id: personId })
  if (error) throw new Error(error.message)
  return data as unknown as FarmerLoginIssued
}

/** Who issued or reset this person's login, newest first. Zero rows: no login yet. */
export async function fetchLoginHistory(personId: string): Promise<LoginHistoryRow[]> {
  const { data, error } = await supabase.rpc('app_person_login_history', { p_person_id: personId })
  // A failed read is not "no login yet": that would offer Create on a person
  // who already has one, and the button would reset their password.
  if (error) throw new Error(error.message)
  return data ?? []
}

export function useLoginHistory(personId: string) {
  return useQuery({
    queryKey: queryKeys.loginHistory(personId),
    queryFn: () => fetchLoginHistory(personId),
  })
}

/**
 * Issue or reset. Invalidates `loginHistory(person)` and nothing else — the
 * narrowest key the call changes (queryKeys invalidation map).
 *
 * `gcTime: 0` drops the finished mutation, and the password inside it, from
 * the mutation cache the moment the card showing it unmounts.
 */
export function useFarmerLoginIssue(personId: string) {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: () => issueFarmerLogin(personId),
    gcTime: 0,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.loginHistory(personId) })
    },
  })
}
