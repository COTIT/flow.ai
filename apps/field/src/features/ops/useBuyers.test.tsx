import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, expect, test, vi } from 'vitest'

const api = { farmMarketBuyersList: vi.fn(), farmMarketBuyersCreate: vi.fn() }
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)
vi.mock('@genesy/api-client/endpoints/farm-market', () => ({
  farmMarketBuyersList: forward('farmMarketBuyersList'),
  farmMarketBuyersCreate: forward('farmMarketBuyersCreate'),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/useOpsReference')
const { queryKeys } = await import('@/lib/queryKeys')

const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})

function client() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue()
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  return { invalidate, wrapper }
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
})

test('buyers are read from the API, every page', async () => {
  const buyer = { id: 'b1', name: 'Iringa Grain', channel: 'direct', contact_note: null, is_active: true }
  api.farmMarketBuyersList.mockResolvedValue(page([buyer]))
  const { wrapper } = client()
  const { result } = renderHook(() => hooks.useBuyers(), { wrapper })

  await waitFor(() => expect(result.current.data).toEqual([buyer]))
  expect(api.farmMarketBuyersList).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
})

test('a new buyer is posted with its draft id, then the buyer lists refresh', async () => {
  api.farmMarketBuyersCreate.mockResolvedValue({ data: { id: 'b9' } })
  const { invalidate, wrapper } = client()
  const { result } = renderHook(() => hooks.useCreateBuyer(), { wrapper })
  const body = { id: 'b9', name: 'Mbeya Millers', channel: 'afm', contact_note: null }

  await result.current.mutateAsync(body)

  expect(api.farmMarketBuyersCreate).toHaveBeenCalledWith('w1', body)
  const keys = invalidate.mock.calls.map((call) => (call[0] as { queryKey: unknown }).queryKey)
  expect(keys).toContainEqual(queryKeys.buyers())
})
