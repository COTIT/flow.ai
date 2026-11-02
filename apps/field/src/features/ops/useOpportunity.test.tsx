import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmMarketOpportunitiesGet: vi.fn(),
  farmMarketHarvestAvailable: vi.fn(),
  farmMarketSupplyCreate: vi.fn(),
  farmMarketOpportunitiesStatus: vi.fn(),
}
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)
vi.mock('@genesy/api-client/endpoints/farm-market', () => ({
  farmMarketOpportunitiesGet: forward('farmMarketOpportunitiesGet'),
  farmMarketHarvestAvailable: forward('farmMarketHarvestAvailable'),
  farmMarketSupplyCreate: forward('farmMarketSupplyCreate'),
  farmMarketOpportunitiesStatus: forward('farmMarketOpportunitiesStatus'),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/useOpportunity')
const { queryKeys } = await import('@/lib/queryKeys')
await import('@/i18n')

const O1 = '40000000-0000-4000-8000-000000000001'
const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})
const opportunityRow = {
  id: O1,
  demand: { id: 'd1', buyer: { id: 'b1', name: 'Iringa Grain' }, quantity_kg: '9000.00', window_start: '2026-09-01', window_end: '2026-09-30' },
  village: { id: 'v1', name: 'Ilundo' },
  crop: { id: 'c1', code: 'MAIZE', name_en: 'Maize', name_sw: 'Mahindi' },
  offered_quantity_kg: '4100.00',
  status: 'proposed',
  note: null,
  captured_at: '2026-10-01T09:00:00Z',
  captured_by: { id: 'u-ops', name: 'Omari Ops' },
  my_contribution_kg: null,
  supply: [
    {
      id: 's1',
      harvest_report_id: 'h1',
      crop_cycle_id: 'cy1',
      plot_id: 'pl1',
      plot_label: 'Kipande cha juu',
      crop_id: 'c1',
      farmer: { person_id: 'p1', name: 'Neema Mwaipopo' },
      contributed_kg: '4100.00',
      captured_by: { id: 'u-ops', name: 'Omari Ops' },
      captured_at: '2026-10-01T09:30:00Z',
    },
  ],
}

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

describe('opportunity reads', () => {
  test('the opportunity with its lines, names and attacher as the API embeds them', async () => {
    api.farmMarketOpportunitiesGet.mockResolvedValue({ data: opportunityRow })
    const { result } = renderHook(() => hooks.useOpportunity(O1), client())

    await waitFor(() => expect(result.current.opportunity).not.toBeNull())
    expect(api.farmMarketOpportunitiesGet).toHaveBeenCalledWith('w1', O1)
    expect(result.current.opportunity).toMatchObject({
      id: O1,
      buyer_demand_id: 'd1',
      village_id: 'v1',
      crop_id: 'c1',
      status: 'proposed',
      buyer_name: 'Iringa Grain',
      village_name: 'Ilundo',
      crop_name: 'Maize',
      demand_quantity_kg: 9000,
      offered_quantity_kg: 4100,
    })
    expect(result.current.opportunity!.supply).toEqual([
      {
        harvest_report_id: 'h1',
        crop_cycle_id: 'cy1',
        contributed_kg: 4100,
        farmer: 'Neema Mwaipopo',
        person_id: 'p1',
        plot_label: 'Kipande cha juu',
        crop_name: 'Maize',
        captured_by: 'Omari Ops',
        captured_at: '2026-10-01T09:30:00Z',
      },
    ])
  })

  test('a 404 or a malformed id is "not found", not an error', async () => {
    api.farmMarketOpportunitiesGet.mockRejectedValue({ response: { status: 404, data: {} } })
    const gone = renderHook(() => hooks.useOpportunity(O1), client())
    await waitFor(() => expect(gone.result.current.isSuccess).toBe(true))
    expect(gone.result.current.opportunity).toBeNull()

    const malformed = renderHook(() => hooks.useOpportunity('nope'), client())
    await waitFor(() => expect(malformed.result.current.isSuccess).toBe(true))
    expect(api.farmMarketOpportunitiesGet).toHaveBeenCalledTimes(1)
  })

  test('available harvest for the village and crop, named by the API', async () => {
    api.farmMarketHarvestAvailable.mockResolvedValue(
      page([
        {
          harvest_report_id: 'h9',
          crop_cycle_id: 'cy9',
          village_id: 'v1',
          crop_id: 'c1',
          plot_id: 'pl9',
          plot_label: 'Shamba la mto',
          farmer: null,
          harvest_start: '2026-09-10',
          harvest_end: null,
          quantity_kg: '5000.00',
          confidence: null,
          verification: 'unverified',
          committed_kg: '0.00',
          available_kg: '5000.00',
        },
      ]),
    )
    const { result } = renderHook(() => hooks.useAvailableHarvest('v1', 'c1'), client())

    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(api.farmMarketHarvestAvailable).toHaveBeenCalledWith('w1', { village_id: 'v1', crop_id: 'c1', page: 1, page_size: 100 })
    expect(result.current.data![0]).toEqual({
      harvest_report_id: 'h9',
      farmer: null,
      plot_label: 'Shamba la mto',
      quantity_kg: 5000,
      available_kg: 5000,
      harvest_start: '2026-09-10',
    })
  })
})

describe('opportunity writes', () => {
  test('supply is posted with its id and the kg as typed, then the opportunity refreshes', async () => {
    api.farmMarketSupplyCreate.mockResolvedValue({ data: opportunityRow })
    const { invalidate, wrapper } = client()
    const { result } = renderHook(() => hooks.useAttachSupply(O1, 'v1'), { wrapper })

    await result.current.mutateAsync({ id: 's9', harvestReportId: 'h9', contributedKg: '1600.50' })

    expect(api.farmMarketSupplyCreate).toHaveBeenCalledWith('w1', O1, {
      id: 's9',
      harvest_report_id: 'h9',
      contributed_kg: '1600.50',
    })
    const keys = invalidate.mock.calls.map((call) => (call[0] as { queryKey?: unknown }).queryKey)
    expect(keys).toContainEqual(queryKeys.opportunity(O1))
  })

  test('a status change is one POST of the target status', async () => {
    api.farmMarketOpportunitiesStatus.mockResolvedValue({ data: opportunityRow })
    const { result } = renderHook(() => hooks.useOpportunityStatus(O1, 'v1', 'd1'), client())

    await result.current.mutateAsync('declined')
    expect(api.farmMarketOpportunitiesStatus).toHaveBeenCalledWith('w1', O1, { status: 'declined' })
  })
})
