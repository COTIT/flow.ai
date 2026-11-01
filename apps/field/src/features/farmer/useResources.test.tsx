import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { expect, test, vi } from 'vitest'

const farmLoansList = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-loans', () => ({
  farmLoansList: (...args: unknown[]) => farmLoansList(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { useLoanProducts } = await import('@/features/farmer/useResources')
await import('@/i18n')

const tzs = (minor: number) => ({ minor, currency: 'TZS', decimals: 2, currency_id: 'c1' })

test('lists the active loan listings, named in the reader’s language, amounts as Money', async () => {
  farmLoansList.mockResolvedValue({
    data: [{
      id: 'l1', code: 'INPUT', name_en: 'Input loan', name_sw: 'Mkopo wa pembejeo',
      description_en: 'Seed and fertiliser.', description_sw: 'Mbegu na mbolea.',
      indicative_min: tzs(20000000), indicative_max: tzs(150000000), is_active: true,
      created_at: '2026-10-01T00:00:00Z', updated_at: '2026-10-01T00:00:00Z',
    }],
    meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: 1, total: 1 },
  })
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  const { result } = renderHook(() => useLoanProducts(), { wrapper })

  await waitFor(() => expect(result.current.items).toEqual([{
    id: 'l1', code: 'INPUT', name: 'Input loan', description: 'Seed and fertiliser.',
    indicative_min: tzs(20000000), indicative_max: tzs(150000000),
  }]))
  expect(farmLoansList).toHaveBeenCalledWith('w1', { active: true, page: 1, page_size: 100 })
})
