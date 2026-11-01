import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const farmVouchersRedemptions = vi.fn()
const farmVouchersRedemptionTotals = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-vouchers', () => ({
  farmVouchersRedemptions: (...args: unknown[]) => farmVouchersRedemptions(...args),
  farmVouchersRedemptionTotals: (...args: unknown[]) => farmVouchersRedemptionTotals(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchRedemptions, useRedemptions } = await import('@/features/ops/surveys/useRedemptions')

const page = (data: unknown[], current: number, last: number) => ({
  data,
  meta: { current_page: current, last_page: last, per_page: 100, from: 1, to: data.length, total: 0 },
})

beforeEach(() => {
  farmVouchersRedemptions.mockReset()
  farmVouchersRedemptionTotals.mockReset()
})

describe('redemptions', () => {
  test('reads totals and the log for the same Tanzanian date range', async () => {
    farmVouchersRedemptionTotals.mockResolvedValue({ data: [{ day: '2026-09-29', vouchers: 2, minor: 1000000 }] })
    farmVouchersRedemptions.mockResolvedValue(page([{ voucher_id: 'v1' }], 1, 1))
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result } = renderHook(() => useRedemptions('2026-09-22', '2026-09-29'), {
      wrapper: ({ children }) => <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>,
    })

    await waitFor(() => expect(result.current.data?.log).toHaveLength(1))
    expect(result.current.data?.totals[0]).toMatchObject({ vouchers: 2, minor: 1000000 })
    expect(farmVouchersRedemptionTotals).toHaveBeenCalledWith('w1', { from: '2026-09-22', to: '2026-09-29' })
    expect(farmVouchersRedemptions).toHaveBeenCalledWith('w1', {
      from: '2026-09-22',
      to: '2026-09-29',
      page: 1,
      page_size: 100,
    })
    expect(queryClient.getQueryData(['redemptionLog', '2026-09-22', '2026-09-29'])).toBeDefined()
  })

  // Cash is counted against the whole range: a partial log would not add up.
  test('reads every page of the log', async () => {
    farmVouchersRedemptionTotals.mockResolvedValue({ data: [] })
    farmVouchersRedemptions
      .mockResolvedValueOnce(page([{ voucher_id: 'v1' }], 1, 2))
      .mockResolvedValueOnce(page([{ voucher_id: 'v2' }], 2, 2))

    const report = await fetchRedemptions('2026-09-22', '2026-09-29')

    expect(report.log.map((row) => row.voucher_id)).toEqual(['v1', 'v2'])
    expect(farmVouchersRedemptions).toHaveBeenLastCalledWith('w1', expect.objectContaining({ page: 2 }))
  })

  test('a failed read is an error', async () => {
    const error = { response: { status: 403, data: { message: 'only ops' } } }
    farmVouchersRedemptionTotals.mockRejectedValue(error)
    farmVouchersRedemptions.mockResolvedValue(page([], 1, 1))
    await expect(fetchRedemptions('2026-09-22', '2026-09-29')).rejects.toBe(error)
  })
})
