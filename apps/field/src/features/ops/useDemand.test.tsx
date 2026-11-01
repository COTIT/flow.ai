import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmMarketDemandsList: vi.fn(),
  farmMarketDemandsGet: vi.fn(),
  farmMarketDemandsCreate: vi.fn(),
  farmMarketDemandMatches: vi.fn(),
  farmMarketOpportunitiesCreate: vi.fn(),
  farmMarketBuyersList: vi.fn(),
  farmCropsList: vi.fn(),
}
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)
vi.mock('@genesy/api-client/endpoints/farm-market', () => ({
  farmMarketDemandsList: forward('farmMarketDemandsList'),
  farmMarketDemandsGet: forward('farmMarketDemandsGet'),
  farmMarketDemandsCreate: forward('farmMarketDemandsCreate'),
  farmMarketDemandMatches: forward('farmMarketDemandMatches'),
  farmMarketOpportunitiesCreate: forward('farmMarketOpportunitiesCreate'),
  farmMarketBuyersList: forward('farmMarketBuyersList'),
}))
vi.mock('@genesy/api-client/endpoints/farm-crops', () => ({ farmCropsList: forward('farmCropsList') }))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/useDemand')
await import('@/i18n')

const D1 = '30000000-0000-4000-8000-000000000001'
const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})
const price = { minor: 80000, decimals: 2, currency: 'TZS', currency_id: 'c-tzs' }
const demandRow = {
  id: D1,
  buyer: { id: 'b1', name: 'Iringa Grain' },
  buyer_channel: 'direct',
  crop: { id: 'c1', code: 'MAIZE', name_en: 'Maize', name_sw: 'Mahindi' },
  quantity_kg: '5000.00',
  window_start: '2026-09-01',
  window_end: '2026-10-31',
  delivery_point: 'Shed',
  quality_note: null,
  indicative_price_per_kg: price,
  status: 'open',
}

function client() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue()
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  return { wrapper }
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
})

describe('demand reads', () => {
  test('the list is the API rows, named, with the price as Money', async () => {
    api.farmMarketDemandsList.mockResolvedValue(page([demandRow]))
    const { result } = renderHook(() => hooks.useDemands(), client())

    await waitFor(() => expect(result.current.demands).toHaveLength(1))
    expect(api.farmMarketDemandsList).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
    expect(result.current.demands[0]).toMatchObject({
      id: D1,
      buyer_name: 'Iringa Grain',
      crop_name: 'Maize',
      quantity_kg: 5000,
      indicative_price_per_kg: price,
      status: 'open',
    })
  })

  test('one demand; a 404 or a malformed id is "not found", not an error', async () => {
    api.farmMarketDemandsGet.mockResolvedValue({ data: demandRow })
    const found = renderHook(() => hooks.useDemand(D1), client())
    await waitFor(() => expect(found.result.current.demand?.buyer_name).toBe('Iringa Grain'))
    expect(api.farmMarketDemandsGet).toHaveBeenCalledWith('w1', D1)

    api.farmMarketDemandsGet.mockRejectedValue({ isAxiosError: true, response: { status: 404, data: {} } })
    const gone = renderHook(() => hooks.useDemand(D1.replace('1', '2')), client())
    await waitFor(() => expect(gone.result.current.isSuccess).toBe(true))
    expect(gone.result.current.demand).toBeNull()

    const malformed = renderHook(() => hooks.useDemand('nope'), client())
    await waitFor(() => expect(malformed.result.current.isSuccess).toBe(true))
    expect(malformed.result.current.demand).toBeNull()
    expect(api.farmMarketDemandsGet).toHaveBeenCalledTimes(2)
  })

  test('matches are the API comparison rows, figures as sent', async () => {
    api.farmMarketDemandMatches.mockResolvedValue({
      data: [
        {
          buyer_demand_id: D1,
          village: { id: 'v1', name: 'Ilundo' },
          demand_kg: '5000.00',
          available_kg: '3200.00',
          committed_kg: '800.00',
          coverable_kg: '3200.00',
          coverage_pct: '64.0',
          opportunity: { id: 'o1', status: 'shared' },
        },
      ],
    })
    const { result } = renderHook(() => hooks.useDemandMatches(D1), client())

    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(api.farmMarketDemandMatches).toHaveBeenCalledWith('w1', D1)
    expect(result.current.data![0]).toEqual({
      village_id: 'v1',
      village_name: 'Ilundo',
      available_kg: 3200,
      committed_kg: 800,
      coverable_kg: 3200,
      coverage_pct: 64,
      opportunity_id: 'o1',
      opportunity_status: 'shared',
    })
  })

  test('the form offers active buyers and active crops', async () => {
    api.farmMarketBuyersList.mockResolvedValue(page([{ id: 'b1', name: 'Iringa Grain' }]))
    api.farmCropsList.mockResolvedValue(
      page([
        { id: 'c1', code: 'MAIZE', name_en: 'Maize', name_sw: 'Mahindi', is_active: true },
        { id: 'c2', code: 'OLD', name_en: 'Old', name_sw: 'Zamani', is_active: false },
      ]),
    )
    const { result } = renderHook(() => hooks.useDemandFormOptions(), client())

    await waitFor(() => expect(result.current.data).toBeDefined())
    expect(api.farmMarketBuyersList).toHaveBeenCalledWith('w1', { active: true, page: 1, page_size: 100 })
    expect(result.current.data).toEqual({
      buyers: [{ id: 'b1', name: 'Iringa Grain' }],
      crops: [{ id: 'c1', name: 'Maize' }],
    })
  })
})

describe('demand writes', () => {
  test('a demand is posted as the form built it', async () => {
    api.farmMarketDemandsCreate.mockResolvedValue({ data: { id: D1 } })
    const { result } = renderHook(() => hooks.useCreateDemand(), client())
    const body = {
      id: D1,
      buyer_id: 'b1',
      crop_id: 'c1',
      quantity_kg: '5000',
      window_start: '2026-09-01',
      window_end: '2026-10-31',
      delivery_point: null,
      quality_note: null,
      indicative_price_per_kg_minor: 80000,
      currency_id: 'c-tzs',
    }

    await result.current.mutateAsync(body)
    expect(api.farmMarketDemandsCreate).toHaveBeenCalledWith('w1', body)
  })

  // A lost reply then a retry must replay the same opportunity, not open a second one.
  test('an opportunity keeps its id until it is saved, then the next one gets a new id', async () => {
    api.farmMarketOpportunitiesCreate.mockRejectedValueOnce(new Error('network'))
    api.farmMarketOpportunitiesCreate.mockResolvedValue({ data: { id: 'o1' } })
    const { result } = renderHook(() => hooks.useCreateOpportunity(D1), client())

    await act(() => result.current.mutateAsync({ villageId: 'v1' }).catch(() => undefined))
    await act(() => result.current.mutateAsync({ villageId: 'v1' }))
    await act(() => result.current.mutateAsync({ villageId: 'v1' }))

    const [first, retry, next] = api.farmMarketOpportunitiesCreate.mock.calls
    expect(first[0]).toBe('w1')
    expect(first[1]).toBe(D1)
    expect(first[2]).toEqual({ id: expect.any(String), village_id: 'v1' })
    expect(retry[2].id).toBe(first[2].id)
    expect(next[2].id).not.toBe(first[2].id)
  })
})
