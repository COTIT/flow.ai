import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { farmLoginsHistory, farmLoginsIssue } from '@genesy/api-client/endpoints/farm-logins'
import type { IssuedLogin, LoginIssue } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { queryKeys } from '@/lib/queryKeys'

/**
 * What `POST …/farm/persons/{person}/login` hands back: the ONLY place the
 * temporary password exists in plain text. It lives in the mutation result and
 * in the component showing it — never a query, never a draft, never a URL,
 * never a log line.
 */
export type FarmerLoginIssued = IssuedLogin

/** One row of the login history. Names and dates, never a password. */
export type LoginHistoryRow = Omit<LoginIssue, 'issued_by'> & { issued_by_name: string }

/**
 * Create a person's app login, or reset its password if one exists — the API
 * decides which, and records every call. The staff-village scope, the phone
 * requirement and the staff-account refusal are all its rules, so none of them
 * is checked here. A refusal rejects with the axios error; its body is the message.
 */
export async function issueFarmerLogin(personId: string): Promise<FarmerLoginIssued> {
  return (await farmLoginsIssue(workspaceId(), personId)).data
}

/**
 * Who issued or reset this person's login, newest first. Zero rows: no login yet.
 * A failed read rejects: it is not "no login yet", which would offer Create on a
 * person who already has one, and the button would reset their password.
 */
export async function fetchLoginHistory(personId: string): Promise<LoginHistoryRow[]> {
  const { data } = await farmLoginsHistory(workspaceId(), personId)
  return data.map(({ issued_by, ...row }) => ({ ...row, issued_by_name: issued_by.name }))
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
