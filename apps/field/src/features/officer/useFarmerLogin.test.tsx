import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const rpc = vi.fn()

vi.mock('@/lib/supabase', () => ({ supabase: { rpc: (...args: unknown[]) => rpc(...args) } }))

const { issueFarmerLogin, fetchLoginHistory, useFarmerLoginIssue, useLoginHistory } = await import(
  '@/features/officer/useFarmerLogin'
)

const PERSON = '11111111-1111-4111-8111-111111111111'

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
}

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
}

beforeEach(() => {
  rpc.mockReset()
})

describe('issueFarmerLogin', () => {
  test('calls the RPC with the person and returns the credential', async () => {
    rpc.mockResolvedValue({
      data: { phone: '+255712000111', temp_password: 'K7QXM2PA', kind: 'initial' },
      error: null,
    })

    await expect(issueFarmerLogin(PERSON)).resolves.toEqual({
      phone: '+255712000111',
      temp_password: 'K7QXM2PA',
      kind: 'initial',
    })
    expect(rpc).toHaveBeenCalledWith('app_farmer_login_issue', { p_person_id: PERSON })
  })

  // Business-rules §9: the database's own sentence, not a rewrite.
  test('a refusal rejects with the database message', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'add a phone number before issuing a login' } })
    await expect(issueFarmerLogin(PERSON)).rejects.toThrow('add a phone number before issuing a login')
  })
})

describe('fetchLoginHistory', () => {
  test('returns the rows newest first, as the database ordered them', async () => {
    const rows = [
      { issued_at: '2026-09-29T09:00:00Z', kind: 'reset', issued_by_name: 'Juma Officer', must_change_password: true },
      { issued_at: '2026-09-28T09:00:00Z', kind: 'initial', issued_by_name: 'Asha Officer', must_change_password: false },
    ]
    rpc.mockResolvedValue({ data: rows, error: null })

    await expect(fetchLoginHistory(PERSON)).resolves.toEqual(rows)
    expect(rpc).toHaveBeenCalledWith('app_person_login_history', { p_person_id: PERSON })
  })

  // Zero rows is an answer: no login yet.
  test('no rows is an empty history, not an error', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    await expect(fetchLoginHistory(PERSON)).resolves.toEqual([])
  })

  test('a failed read rejects rather than reading as "no login"', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'history failed' } })
    await expect(fetchLoginHistory(PERSON)).rejects.toThrow('history failed')
  })
})

describe('useLoginHistory', () => {
  test('reads under loginHistory(person)', async () => {
    rpc.mockResolvedValue({ data: [], error: null })
    const queryClient = client()
    const { result } = renderHook(() => useLoginHistory(PERSON), { wrapper: wrapper(queryClient) })

    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(queryClient.getQueryData(['loginHistory', PERSON])).toEqual([])
  })
})

describe('useFarmerLoginIssue', () => {
  test('invalidates only the login history, and never caches the password as a query', async () => {
    rpc.mockResolvedValue({
      data: { phone: '+255712000111', temp_password: 'K7QXM2PA', kind: 'reset' },
      error: null,
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

  test('a failure is the mutation error, with the database message', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'this phone number already has an app login' } })
    const queryClient = client()
    const { result } = renderHook(() => useFarmerLoginIssue(PERSON), { wrapper: wrapper(queryClient) })

    await expect(result.current.mutateAsync()).rejects.toThrow('this phone number already has an app login')
  })
})
