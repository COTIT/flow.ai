import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { farmPassbookMine, farmPassbookPerson } from '@genesy/api-client/endpoints/farm-passbook'
import { farmPayoutsCreate, farmPayoutsList } from '@genesy/api-client/endpoints/farm-payouts'
import type { CreatePayoutRequest, EntryResource, Money, PassbookPage } from '@genesy/api-client/model'

import { workspaceId } from '@/app/workspace'
import { allPages } from '@/features/farmer/pages'
import { queryKeys } from '@/lib/queryKeys'

/**
 * A farmer's passbook (money lines, newest first) and payouts. Every figure is the API's:
 * signed line amounts, the balance after each line and the balance per currency. The browser
 * formats them and adds nothing up. Who may read or pay is the API's to refuse.
 */

export interface Passbook {
  lines: EntryResource[]
  /** Per currency, over every line (the API's sum, not the lines read). */
  balances: Money[]
}

/** Every page of a passbook; the balances come with each page, so the first page's are kept. */
async function readPassbook(read: (params: { page: number; page_size: number }) => Promise<PassbookPage>): Promise<Passbook> {
  let balances: Money[] = []
  const lines = await allPages(async (params) => {
    const page = await read(params)
    if (params.page === 1) balances = page.balances
    return page
  })
  return { lines, balances }
}

export function useMyPassbook() {
  return useQuery({
    queryKey: queryKeys.passbook('mine'),
    queryFn: () => readPassbook((p) => farmPassbookMine(workspaceId(), p)),
  })
}

export function usePersonPassbook(personId: string) {
  return useQuery({
    queryKey: queryKeys.passbook(personId),
    queryFn: () => readPassbook((p) => farmPassbookPerson(workspaceId(), personId, p)),
  })
}

/** One person's payouts, or with no person every payout the caller may read (a farmer: their own). */
export function usePayouts(personId?: string) {
  return useQuery({
    queryKey: queryKeys.payouts(personId ?? 'all'),
    queryFn: () =>
      allPages((p) => farmPayoutsList(workspaceId(), personId ? { person_id: personId, ...p } : p)),
  })
}

/** Pay a farmer cash. The body's `client_ref` is the retry key: a resend replays, never pays twice. */
export function useCreatePayout() {
  const client = useQueryClient()
  return useMutation({
    mutationFn: async (body: CreatePayoutRequest) => (await farmPayoutsCreate(workspaceId(), body)).data,
    onSuccess: () =>
      Promise.all([
        client.invalidateQueries({ queryKey: ['passbook'] }),
        client.invalidateQueries({ queryKey: ['payouts'] }),
      ]),
  })
}
