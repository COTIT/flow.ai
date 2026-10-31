import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const farmVouchersMine = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-vouchers', () => ({
  farmVouchersMine: (...args: unknown[]) => farmVouchersMine(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchFarmerVouchers, redeemedByName, useVoucher, voucherDisplayStatus } = await import(
  '@/features/farmer/useVouchers'
)

const VOUCHER = '51000000-0000-4000-8000-000000000001'
const OTHER = '51000000-0000-4000-8000-000000000002'

/** One row of GET /farm/vouchers/mine. */
const voucher = (over: Record<string, unknown> = {}) => ({
  id: VOUCHER,
  survey_id: 's1',
  survey_title_en: 'Maize storage',
  survey_title_sw: null,
  code: 'K7QXM2PA9D',
  amount: { minor: 500000, decimals: 2, currency: 'TZS', currency_id: 'c-tzs' },
  status: 'issued',
  expired: false,
  issued_at: '2026-09-20T08:00:00Z',
  expires_at: '2026-10-20T08:00:00Z',
  redeemed_at: null,
  ...over,
})
const page = (data: unknown[], current_page: number, last_page: number) => ({
  data,
  meta: { current_page, last_page, per_page: 100, from: 1, to: data.length, total: 0 },
})

beforeEach(() => {
  farmVouchersMine.mockReset()
})

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
}

describe('the household vouchers', () => {
  // The API scopes /vouchers/mine to the caller's household; nothing is filtered here.
  test('reads every page of vouchers/mine, codes as the API sends them', async () => {
    farmVouchersMine
      .mockResolvedValueOnce(page([voucher()], 1, 2))
      .mockResolvedValueOnce(page([voucher({ id: OTHER, code: 'AAAAA11111' })], 2, 2))

    const rows = await fetchFarmerVouchers()

    expect(farmVouchersMine).toHaveBeenNthCalledWith(1, 'w1', { page: 1, page_size: 100 })
    expect(farmVouchersMine).toHaveBeenNthCalledWith(2, 'w1', { page: 2, page_size: 100 })
    expect(rows.map((r) => r.code)).toEqual(['K7QXM2PA9D', 'AAAAA11111'])
  })

  test('a failed read rejects rather than reading as no vouchers', async () => {
    const error = { response: { status: 500 } }
    farmVouchersMine.mockRejectedValue(error)
    await expect(fetchFarmerVouchers()).rejects.toBe(error)
  })

  test('one voucher is picked out of the household list by id', async () => {
    farmVouchersMine.mockResolvedValue(page([voucher({ id: OTHER }), voucher()], 1, 1))
    const { result } = renderHook(() => useVoucher(VOUCHER), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data?.id).toBe(VOUCHER)
  })

  test('a voucher that is not in the list is null, not an error', async () => {
    farmVouchersMine.mockResolvedValue(page([voucher({ id: OTHER })], 1, 1))
    const { result } = renderHook(() => useVoucher(VOUCHER), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBeNull()
  })
})

test('the name on the hand-over is the latest redeemed event', () => {
  expect(
    redeemedByName([
      { occurred_at: '1', kind: 'issued', actor_name: 'Amina', actor_role: 'farmer', detail: null },
      { occurred_at: '2', kind: 'redeemed', actor_name: 'Juma Officer', actor_role: 'field_officer', detail: null },
    ]),
  ).toBe('Juma Officer')
  expect(redeemedByName([])).toBeNull()
})

/** `expired` is the API's verdict (issued and past expires_at); nothing is re-derived from the clock here. */
describe('the status a voucher is shown with', () => {
  test('issued and in date', () => {
    expect(voucherDisplayStatus({ status: 'issued', expired: false })).toBe('issued')
  })

  test('issued and expired by the API is expired', () => {
    expect(voucherDisplayStatus({ status: 'issued', expired: true })).toBe('expired')
  })

  test('collected and cancelled keep the status that says what happened', () => {
    expect(voucherDisplayStatus({ status: 'redeemed', expired: false })).toBe('redeemed')
    expect(voucherDisplayStatus({ status: 'void', expired: false })).toBe('void')
  })
})
