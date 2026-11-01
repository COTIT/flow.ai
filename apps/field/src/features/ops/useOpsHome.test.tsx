import { beforeEach, expect, test, vi } from 'vitest'

const pue = vi.fn()
const demands = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-pue', () => ({ farmPueRequestsList: (...a: unknown[]) => pue(...a) }))
vi.mock('@genesy/api-client/endpoints/farm-market', () => ({ farmMarketDemandsList: (...a: unknown[]) => demands(...a) }))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))
vi.mock('@/lib/supabase', () => {
  throw new Error('the ops home must not reach Supabase')
})

const { fetchOpsQueues } = await import('@/features/ops/useOpsHome')

const total = (n: number) => ({ data: [], meta: { current_page: 1, last_page: 1, per_page: 1, from: null, to: null, total: n } })

beforeEach(() => {
  pue.mockReset()
  demands.mockReset()
})

test('the queue counts are the API totals, one row asked for', async () => {
  pue.mockResolvedValue(total(2))
  demands.mockResolvedValue(total(1))

  await expect(fetchOpsQueues()).resolves.toEqual({ awaitingReview: 2, openDemands: 1 })
  expect(pue).toHaveBeenCalledWith('w1', { status: 'submitted,under_review', page_size: 1 })
  expect(demands).toHaveBeenCalledWith('w1', { status: 'open', page_size: 1 })
})

test('a failed count is not zero', async () => {
  pue.mockResolvedValue(total(2))
  demands.mockRejectedValue(new Error('Forbidden'))
  await expect(fetchOpsQueues()).rejects.toThrow('Forbidden')
})
