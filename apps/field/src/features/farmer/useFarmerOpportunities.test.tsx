import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, expect, test, vi } from 'vitest'

const farmMarketOpportunitiesList = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-market', () => ({
  farmMarketOpportunitiesList: (...args: unknown[]) => farmMarketOpportunitiesList(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { useFarmerOpportunities } = await import('@/features/farmer/useFarmerOpportunities')
await import('@/i18n')

const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

beforeEach(() => farmMarketOpportunitiesList.mockReset())

// The API scopes the list to opportunities holding this farmer's harvest and works out
// my_contribution_kg from their own lines; the hook shows its figures and adds nothing up.
test('lists the API rows with the farmer share the API worked out', async () => {
  farmMarketOpportunitiesList.mockResolvedValue(
    page([
      {
        id: 'o1',
        status: 'shared',
        crop: { id: 'c1', code: 'MAIZE', name_en: 'Maize', name_sw: 'Mahindi' },
        offered_quantity_kg: '6400.00',
        my_contribution_kg: '4100.00',
        supply: [{ id: 's1', contributed_kg: '4100.00' }],
      },
      { id: 'o2', status: 'proposed', crop: { id: 'c1', code: 'MAIZE', name_en: 'Maize', name_sw: 'Mahindi' }, offered_quantity_kg: '0', my_contribution_kg: null, supply: [] },
    ]),
  )
  const { result } = renderHook(() => useFarmerOpportunities(), { wrapper })

  await waitFor(() => expect(result.current.data).toHaveLength(2))
  expect(farmMarketOpportunitiesList).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
  expect(result.current.data![0]).toMatchObject({
    id: 'o1',
    status: 'shared',
    crop_name: 'Maize',
    offered_quantity_kg: 6400,
    my_contribution_kg: 4100,
  })
  // No person record: the share is unknown, not zero.
  expect(result.current.data![1].my_contribution_kg).toBeNull()
})
