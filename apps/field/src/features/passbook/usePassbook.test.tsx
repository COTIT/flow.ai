import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const farmPassbookMine = vi.fn()
const farmPassbookPerson = vi.fn()
const farmPayoutsList = vi.fn()
const farmPayoutsCreate = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-passbook', () => ({
  farmPassbookMine: (...a: unknown[]) => farmPassbookMine(...a),
  farmPassbookPerson: (...a: unknown[]) => farmPassbookPerson(...a),
}))
vi.mock('@genesy/api-client/endpoints/farm-payouts', () => ({
  farmPayoutsList: (...a: unknown[]) => farmPayoutsList(...a),
  farmPayoutsCreate: (...a: unknown[]) => farmPayoutsCreate(...a),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/passbook/usePassbook')

const tzs = (minor: number) => ({ minor, currency: 'TZS', decimals: 2, currency_id: 'tzs' })
const line = (id: string, minor: number) => ({ id, kind: 'sale_credit', amount: tzs(minor), balance_after: tzs(minor), attribution_id: null, payout_id: null, correction_id: null, reason: null, created_at: '2026-11-01T08:00:00Z' })
const meta = (page: number, last: number) => ({ current_page: page, last_page: last })

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>
}

beforeEach(() => vi.clearAllMocks())

describe('passbook reads', () => {
  test("my passbook: every page's lines, the balances as the API sums them", async () => {
    farmPassbookMine
      .mockResolvedValueOnce({ data: [line('a', 500)], meta: meta(1, 2), balances: [tzs(800)] })
      .mockResolvedValueOnce({ data: [line('b', 300)], meta: meta(2, 2), balances: [tzs(800)] })
    const { result } = renderHook(() => hooks.useMyPassbook(), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.data?.lines.map((l) => l.id)).toEqual(['a', 'b']))
    expect(result.current.data?.balances).toEqual([tzs(800)])
    expect(farmPassbookMine).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
    expect(farmPassbookMine).toHaveBeenLastCalledWith('w1', { page: 2, page_size: 100 })
  })

  test("a person's passbook is read by person", async () => {
    farmPassbookPerson.mockResolvedValue({ data: [], meta: meta(1, 1), balances: [] })
    const { result } = renderHook(() => hooks.usePersonPassbook('p1'), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.data).toEqual({ lines: [], balances: [] }))
    expect(farmPassbookPerson).toHaveBeenCalledWith('w1', 'p1', { page: 1, page_size: 100 })
  })

  test('payouts: of one person, or (no person) whatever the API lets the caller see', async () => {
    farmPayoutsList.mockResolvedValue({ data: [{ id: 'po1' }], meta: meta(1, 1) })
    const one = renderHook(() => hooks.usePayouts('p1'), { wrapper: wrapper() })
    await waitFor(() => expect(one.result.current.data).toEqual([{ id: 'po1' }]))
    expect(farmPayoutsList).toHaveBeenCalledWith('w1', { person_id: 'p1', page: 1, page_size: 100 })
    const mine = renderHook(() => hooks.usePayouts(), { wrapper: wrapper() })
    await waitFor(() => expect(mine.result.current.data).toEqual([{ id: 'po1' }]))
    expect(farmPayoutsList).toHaveBeenLastCalledWith('w1', { page: 1, page_size: 100 })
  })
})

describe('paying out', () => {
  test('posts the body as built', async () => {
    farmPayoutsCreate.mockResolvedValue({ data: { id: 'po9' } })
    const body = { client_ref: 'c1', person_id: 'p1', amount: 300000, currency_id: 'tzs', id_type: 'nida', name_confirmed: true }
    const { result } = renderHook(() => hooks.useCreatePayout(), { wrapper: wrapper() })
    await act(async () => {
      await result.current.mutateAsync(body)
    })
    expect(farmPayoutsCreate).toHaveBeenCalledWith('w1', body)
  })
})
