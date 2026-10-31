import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const farmVouchersLookup = vi.fn()
const farmVouchersRedeem = vi.fn()
const farmVouchersTimeline = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-vouchers', () => ({
  farmVouchersLookup: (...args: unknown[]) => farmVouchersLookup(...args),
  farmVouchersRedeem: (...args: unknown[]) => farmVouchersRedeem(...args),
  farmVouchersTimeline: (...args: unknown[]) => farmVouchersTimeline(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const {
  lookupVoucher,
  redeemVoucher,
  fetchVoucherTimeline,
  useVoucherLookup,
  useVoucherRedeem,
  useVoucherTimeline,
} = await import('@/features/officer/useVoucherRedeem')

const VOUCHER = '22222222-2222-4222-8222-222222222222'
const AMOUNT = { minor: 500000, decimals: 2, currency: 'TZS', currency_id: 'c-tzs' }
const failed = (status: number, message: string) => ({ response: { status, data: { message } } })

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
}

function wrapper(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
}

beforeEach(() => {
  farmVouchersLookup.mockReset()
  farmVouchersRedeem.mockReset()
  farmVouchersTimeline.mockReset()
})

describe('lookupVoucher', () => {
  test('sends the code and returns the preview', async () => {
    farmVouchersLookup.mockResolvedValue({ data: { found: true, voucher_id: VOUCHER, can_redeem: true } })

    await expect(lookupVoucher('K7QXM2PA9D')).resolves.toMatchObject({ found: true, voucher_id: VOUCHER })
    expect(farmVouchersLookup).toHaveBeenCalledWith('w1', { code: 'K7QXM2PA9D' })
  })

  // Unknown and out-of-village are the same answer on purpose.
  test('found:false is an answer, not an error', async () => {
    farmVouchersLookup.mockResolvedValue({ data: { found: false } })
    await expect(lookupVoucher('K7QXM2PA9D')).resolves.toEqual({ found: false })
  })

  test('a refusal rejects with the API error', async () => {
    const error = failed(429, 'Too many requests.')
    farmVouchersLookup.mockRejectedValue(error)
    await expect(lookupVoucher('K7QXM2PA9D')).rejects.toBe(error)
  })
})

describe('redeemVoucher', () => {
  test('sends the code, the ID seen and the name confirmation', async () => {
    farmVouchersRedeem.mockResolvedValue({
      data: { voucher_id: VOUCHER, status: 'redeemed', amount: AMOUNT, replayed: false },
    })

    await expect(
      redeemVoucher({ code: 'K7QXM2PA9D', idType: 'nida', nameConfirmed: true }),
    ).resolves.toMatchObject({ voucher_id: VOUCHER, status: 'redeemed' })
    expect(farmVouchersRedeem).toHaveBeenCalledWith('w1', {
      code: 'K7QXM2PA9D',
      id_type: 'nida',
      name_confirmed: true,
    })
  })

  // The API refuses a missing ID type with its own sentence, so the client
  // passes the gap through rather than pre-validating it.
  test('an unchosen ID type is sent blank for the API to refuse', async () => {
    const error = failed(422, 'record which ID document you checked')
    farmVouchersRedeem.mockRejectedValue(error)

    await expect(redeemVoucher({ code: 'K7QXM2PA9D', idType: null, nameConfirmed: false })).rejects.toBe(error)
    expect(farmVouchersRedeem).toHaveBeenCalledWith('w1', {
      code: 'K7QXM2PA9D',
      id_type: '',
      name_confirmed: false,
    })
  })
})

describe('fetchVoucherTimeline', () => {
  test('returns the rows the API filtered for this caller', async () => {
    const rows = [
      { occurred_at: '2026-09-29T08:00:00Z', kind: 'issued', actor_name: 'Neema', actor_role: 'farmer', detail: {} },
    ]
    farmVouchersTimeline.mockResolvedValue({ data: rows })

    await expect(fetchVoucherTimeline(VOUCHER)).resolves.toEqual(rows)
    expect(farmVouchersTimeline).toHaveBeenCalledWith('w1', VOUCHER)
  })
})

describe('useVoucherLookup', () => {
  // Every lookup writes a 'scanned' audit event. A query could refetch on
  // focus or remount and write events nobody performed, so it is a mutation.
  test('is a mutation: it runs only when asked, and never lands in the query cache', async () => {
    farmVouchersLookup.mockResolvedValue({ data: { found: false } })
    const queryClient = client()
    const { result } = renderHook(() => useVoucherLookup(), { wrapper: wrapper(queryClient) })

    expect(farmVouchersLookup).not.toHaveBeenCalled()
    await expect(result.current.mutateAsync('K7QXM2PA9D')).resolves.toEqual({ found: false })
    expect(farmVouchersLookup).toHaveBeenCalledTimes(1)
    expect(queryClient.getQueryCache().getAll()).toHaveLength(0)
  })
})

describe('useVoucherRedeem', () => {
  test('invalidates the voucher trail and the ops views that count it', async () => {
    farmVouchersRedeem.mockResolvedValue({
      data: { voucher_id: VOUCHER, status: 'redeemed', amount: AMOUNT, replayed: false },
    })
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const { result } = renderHook(() => useVoucherRedeem(), { wrapper: wrapper(queryClient) })

    await result.current.mutateAsync({ code: 'K7QXM2PA9D', idType: 'voter', nameConfirmed: true })

    const keys = invalidate.mock.calls.map(([filters]) => (filters as { queryKey: unknown[] }).queryKey)
    expect(keys).toEqual(
      expect.arrayContaining([
        ['voucherTimeline', VOUCHER],
        ['surveyVouchers'],
        ['surveyAdmin'],
        ['redemptionLog'],
      ]),
    )
    expect(keys).toHaveLength(4)
  })

  test('a refusal is the mutation error, as the API sent it', async () => {
    const error = failed(422, 'you registered this household, so another staff member must redeem this voucher')
    farmVouchersRedeem.mockRejectedValue(error)
    const { result } = renderHook(() => useVoucherRedeem(), { wrapper: wrapper(client()) })

    await expect(
      result.current.mutateAsync({ code: 'K7QXM2PA9D', idType: 'nida', nameConfirmed: true }),
    ).rejects.toBe(error)
  })
})

describe('useVoucherTimeline', () => {
  test('waits for a voucher, then reads under voucherTimeline(id)', async () => {
    farmVouchersTimeline.mockResolvedValue({ data: [] })
    const queryClient = client()
    const { result, rerender } = renderHook(({ id }: { id: string | null }) => useVoucherTimeline(id), {
      wrapper: wrapper(queryClient),
      initialProps: { id: null as string | null },
    })

    expect(result.current.fetchStatus).toBe('idle')
    expect(farmVouchersTimeline).not.toHaveBeenCalled()

    rerender({ id: VOUCHER })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(queryClient.getQueryData(['voucherTimeline', VOUCHER])).toEqual([])
  })
})
