import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const rpc = vi.fn()

vi.mock('@/lib/supabase', () => ({ supabase: { rpc: (...args: unknown[]) => rpc(...args) } }))

const {
  lookupVoucher,
  redeemVoucher,
  fetchVoucherTimeline,
  useVoucherLookup,
  useVoucherRedeem,
  useVoucherTimeline,
} = await import('@/features/officer/useVoucherRedeem')

const VOUCHER = '22222222-2222-4222-8222-222222222222'

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
}

function wrapper(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
}

beforeEach(() => {
  rpc.mockReset()
})

describe('lookupVoucher', () => {
  test('sends the normalised code and returns the preview', async () => {
    rpc.mockResolvedValue({ data: { found: true, voucher_id: VOUCHER, can_redeem: true }, error: null })

    await expect(lookupVoucher('K7QXM2PA9D')).resolves.toMatchObject({ found: true, voucher_id: VOUCHER })
    expect(rpc).toHaveBeenCalledWith('app_voucher_lookup', { p_code: 'K7QXM2PA9D' })
  })

  // Unknown and out-of-village are the same answer on purpose.
  test('found:false is an answer, not an error', async () => {
    rpc.mockResolvedValue({ data: { found: false }, error: null })
    await expect(lookupVoucher('K7QXM2PA9D')).resolves.toEqual({ found: false })
  })

  test('a refusal rejects with the database message', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'only staff may look up vouchers' } })
    await expect(lookupVoucher('K7QXM2PA9D')).rejects.toThrow('only staff may look up vouchers')
  })
})

describe('redeemVoucher', () => {
  test('sends the code, the ID seen and the name confirmation', async () => {
    rpc.mockResolvedValue({ data: { voucher_id: VOUCHER, status: 'redeemed', replayed: false }, error: null })

    await expect(
      redeemVoucher({ code: 'K7QXM2PA9D', idType: 'nida', nameConfirmed: true }),
    ).resolves.toMatchObject({ voucher_id: VOUCHER, status: 'redeemed' })
    expect(rpc).toHaveBeenCalledWith('app_voucher_redeem', {
      p_code: 'K7QXM2PA9D',
      p_id_type: 'nida',
      p_name_confirmed: true,
    })
  })

  // The database refuses a missing ID type with its own sentence, so the
  // client passes the gap through rather than pre-validating it.
  test('an unchosen ID type is sent as null for the database to refuse', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'record which ID document you checked' } })

    await expect(
      redeemVoucher({ code: 'K7QXM2PA9D', idType: null, nameConfirmed: false }),
    ).rejects.toThrow('record which ID document you checked')
    expect(rpc).toHaveBeenCalledWith('app_voucher_redeem', {
      p_code: 'K7QXM2PA9D',
      p_id_type: null,
      p_name_confirmed: false,
    })
  })
})

describe('fetchVoucherTimeline', () => {
  test('returns the rows the database filtered for this caller', async () => {
    const rows = [
      { occurred_at: '2026-09-29T08:00:00Z', kind: 'issued', actor_name: 'Neema', actor_role: 'farmer', detail: null },
    ]
    rpc.mockResolvedValue({ data: rows, error: null })

    await expect(fetchVoucherTimeline(VOUCHER)).resolves.toEqual(rows)
    expect(rpc).toHaveBeenCalledWith('app_voucher_timeline', { p_voucher_id: VOUCHER })
  })

  test('no rows is an empty trail; a failed read rejects', async () => {
    rpc.mockResolvedValueOnce({ data: null, error: null })
    await expect(fetchVoucherTimeline(VOUCHER)).resolves.toEqual([])

    rpc.mockResolvedValueOnce({ data: null, error: { message: 'timeline failed' } })
    await expect(fetchVoucherTimeline(VOUCHER)).rejects.toThrow('timeline failed')
  })
})

describe('useVoucherLookup', () => {
  // Every lookup writes a 'scanned' audit event. A query could refetch on
  // focus or remount and write events nobody performed, so it is a mutation.
  test('is a mutation: it runs only when asked, and never lands in the query cache', async () => {
    rpc.mockResolvedValue({ data: { found: false }, error: null })
    const queryClient = client()
    const { result } = renderHook(() => useVoucherLookup(), { wrapper: wrapper(queryClient) })

    expect(rpc).not.toHaveBeenCalled()
    await expect(result.current.mutateAsync('K7QXM2PA9D')).resolves.toEqual({ found: false })
    expect(rpc).toHaveBeenCalledTimes(1)
    expect(queryClient.getQueryCache().getAll()).toHaveLength(0)
  })
})

describe('useVoucherRedeem', () => {
  test('invalidates the voucher trail and the ops views that count it', async () => {
    rpc.mockResolvedValue({ data: { voucher_id: VOUCHER, status: 'redeemed', replayed: false }, error: null })
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

  test('a refusal is the mutation error, verbatim', async () => {
    rpc.mockResolvedValue({
      data: null,
      error: { message: 'you registered this household, so another staff member must redeem this voucher' },
    })
    const { result } = renderHook(() => useVoucherRedeem(), { wrapper: wrapper(client()) })

    await expect(
      result.current.mutateAsync({ code: 'K7QXM2PA9D', idType: 'nida', nameConfirmed: true }),
    ).rejects.toThrow('you registered this household, so another staff member must redeem this voucher')
  })
})

describe('useVoucherTimeline', () => {
  test('waits for a voucher, then reads under voucherTimeline(id)', async () => {
    rpc.mockResolvedValue({ data: [], error: null })
    const queryClient = client()
    const { result, rerender } = renderHook(({ id }: { id: string | null }) => useVoucherTimeline(id), {
      wrapper: wrapper(queryClient),
      initialProps: { id: null as string | null },
    })

    expect(result.current.fetchStatus).toBe('idle')
    expect(rpc).not.toHaveBeenCalled()

    rerender({ id: VOUCHER })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(queryClient.getQueryData(['voucherTimeline', VOUCHER])).toEqual([])
  })
})
