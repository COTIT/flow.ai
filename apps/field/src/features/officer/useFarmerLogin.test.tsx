import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { sessionStore } from '@genesy/api-client/session'

const farmLoginsIssue = vi.fn()
const farmLoginsHistory = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-logins', () => ({
  farmLoginsIssue: (...args: unknown[]) => farmLoginsIssue(...args),
  farmLoginsHistory: (...args: unknown[]) => farmLoginsHistory(...args),
}))

const { issueFarmerLogin, fetchLoginHistory, useFarmerLoginIssue, useLoginHistory } = await import(
  '@/features/officer/useFarmerLogin'
)

const PERSON = '11111111-1111-4111-8111-111111111111'
const WORKSPACE = '20000000-0000-4000-8000-000000000001'
/** An API refusal as axios hands it over. */
const refused = (message: string) => ({ response: { status: 422, data: { message } } })

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
}

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
}

beforeEach(() => {
  farmLoginsIssue.mockReset()
  farmLoginsHistory.mockReset()
  sessionStore.getState().signIn({ access_token: 'tok', token_type: 'Bearer', workspace_ids: [WORKSPACE] })
})

describe('issueFarmerLogin', () => {
  test('posts for the person and returns the credential', async () => {
    farmLoginsIssue.mockResolvedValue({
      data: { phone: '+255712000111', temp_password: 'K7QXM2PA', kind: 'initial' },
    })

    await expect(issueFarmerLogin(PERSON)).resolves.toEqual({
      phone: '+255712000111',
      temp_password: 'K7QXM2PA',
      kind: 'initial',
    })
    expect(farmLoginsIssue).toHaveBeenCalledWith(WORKSPACE, PERSON)
  })

  // Business-rules §9: the API's own sentence, not a rewrite.
  test('a refusal rejects with the API error, carrying its message', async () => {
    farmLoginsIssue.mockRejectedValue(refused('add a phone number before issuing a login'))
    await expect(issueFarmerLogin(PERSON)).rejects.toEqual(refused('add a phone number before issuing a login'))
  })
})

describe('fetchLoginHistory', () => {
  test('returns the rows newest first, as the API ordered them, with the issuer name', async () => {
    farmLoginsHistory.mockResolvedValue({
      data: [
        { issued_at: '2026-09-29T09:00:00Z', kind: 'reset', issued_by: { id: 'u2', name: 'Juma Officer' }, must_change_password: true },
        { issued_at: '2026-09-28T09:00:00Z', kind: 'initial', issued_by: { id: 'u1', name: 'Asha Officer' }, must_change_password: false },
      ],
    })

    await expect(fetchLoginHistory(PERSON)).resolves.toEqual([
      { issued_at: '2026-09-29T09:00:00Z', kind: 'reset', issued_by_name: 'Juma Officer', must_change_password: true },
      { issued_at: '2026-09-28T09:00:00Z', kind: 'initial', issued_by_name: 'Asha Officer', must_change_password: false },
    ])
    expect(farmLoginsHistory).toHaveBeenCalledWith(WORKSPACE, PERSON)
  })

  // Zero rows is an answer: no login yet.
  test('no rows is an empty history, not an error', async () => {
    farmLoginsHistory.mockResolvedValue({ data: [] })
    await expect(fetchLoginHistory(PERSON)).resolves.toEqual([])
  })

  test('a failed read rejects rather than reading as "no login"', async () => {
    farmLoginsHistory.mockRejectedValue(refused('history failed'))
    await expect(fetchLoginHistory(PERSON)).rejects.toEqual(refused('history failed'))
  })
})

describe('useLoginHistory', () => {
  test('reads under loginHistory(person)', async () => {
    farmLoginsHistory.mockResolvedValue({ data: [] })
    const queryClient = client()
    const { result } = renderHook(() => useLoginHistory(PERSON), { wrapper: wrapper(queryClient) })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(queryClient.getQueryData(['loginHistory', PERSON])).toEqual([])
  })
})

describe('useFarmerLoginIssue', () => {
  test('invalidates only the login history, and never caches the password as a query', async () => {
    farmLoginsIssue.mockResolvedValue({
      data: { phone: '+255712000111', temp_password: 'K7QXM2PA', kind: 'reset' },
    })
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const { result } = renderHook(() => useFarmerLoginIssue(PERSON), { wrapper: wrapper(queryClient) })

    await expect(result.current.mutateAsync()).resolves.toMatchObject({ temp_password: 'K7QXM2PA' })

    expect(invalidate).toHaveBeenCalledTimes(1)
    expect(invalidate).toHaveBeenCalledWith({ queryKey: ['loginHistory', PERSON] })
    // The temporary password exists in the mutation's result only: nothing in
    // the query cache holds it, so no refetch or devtool can surface it later.
    const cached = JSON.stringify(queryClient.getQueryCache().getAll().map((q) => q.state.data))
    expect(cached).not.toContain('K7QXM2PA')
  })

  test('a failure is the mutation error, with the API message', async () => {
    farmLoginsIssue.mockRejectedValue(refused('this phone number already has an app login'))
    const queryClient = client()
    const { result } = renderHook(() => useFarmerLoginIssue(PERSON), { wrapper: wrapper(queryClient) })

    await expect(result.current.mutateAsync()).rejects.toEqual(refused('this phone number already has an app login'))
  })
})
