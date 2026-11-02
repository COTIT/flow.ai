import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmEquipmentCategoriesList: vi.fn(),
  farmEquipmentCategoriesCreate: vi.fn(),
  farmEquipmentCreate: vi.fn(),
  farmLoansCreate: vi.fn(),
}
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)
vi.mock('@genesy/api-client/endpoints/farm-equipment', () => ({
  farmEquipmentCategoriesList: forward('farmEquipmentCategoriesList'),
  farmEquipmentCategoriesCreate: forward('farmEquipmentCategoriesCreate'),
  farmEquipmentCreate: forward('farmEquipmentCreate'),
}))
vi.mock('@genesy/api-client/endpoints/farm-loans', () => ({ farmLoansCreate: forward('farmLoansCreate') }))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/useCatalogue')
const { queryKeys } = await import('@/lib/queryKeys')
await import('@/i18n')

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
const keys = (invalidate: { mock: { calls: unknown[][] } }) =>
  invalidate.mock.calls.map((call) => (call[0] as { queryKey: unknown }).queryKey)

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
})

describe('equipment categories', () => {
  test('lists the active ones, named in the reader’s language', async () => {
    api.farmEquipmentCategoriesList.mockResolvedValue(
      page([{ id: 'cat1', code: 'MILLING', name_en: 'Milling', name_sw: 'Kusaga', is_active: true }]),
    )
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useEquipmentCategories(), { wrapper })

    await waitFor(() => expect(result.current.data).toEqual([{ id: 'cat1', code: 'MILLING', name: 'Milling' }]))
    expect(api.farmEquipmentCategoriesList).toHaveBeenCalledWith('w1', { active: true, page: 1, page_size: 100 })
  })

  test('a new category is posted with its device-made id, then the list refreshes', async () => {
    api.farmEquipmentCategoriesCreate.mockResolvedValue({ data: { id: 'cat9' } })
    const { invalidate, wrapper } = client()
    const { result } = renderHook(() => hooks.useCreateCategory(), { wrapper })
    const body = { id: 'cat9', code: 'COLD', name_en: 'Cold storage', name_sw: 'Hifadhi baridi' }

    await result.current.mutateAsync(body)

    expect(api.farmEquipmentCategoriesCreate).toHaveBeenCalledWith('w1', body)
    expect(keys(invalidate)).toContainEqual(queryKeys.equipmentCategories())
  })
})

describe('a new machine', () => {
  test('is posted as typed, with its id, then the equipment list refreshes', async () => {
    api.farmEquipmentCreate.mockResolvedValue({ data: { id: 'eq9' } })
    const { invalidate, wrapper } = client()
    const { result } = renderHook(() => hooks.useCreateEquipment(), { wrapper })
    const body = {
      id: 'eq9',
      category_id: 'cat1',
      code: 'HAMMER',
      name_en: 'Hammer mill',
      name_sw: 'Mashine ya nyundo',
      rated_power_kw: '5.5',
      can_rent: true,
      can_buy: false,
      indicative_rent_per_day_minor: 6000000,
      currency_id: 'c1',
    }

    await result.current.mutateAsync(body)

    expect(api.farmEquipmentCreate).toHaveBeenCalledWith('w1', body)
    expect(keys(invalidate)).toContainEqual(queryKeys.equipment('mine'))
  })
})

describe('a new loan listing', () => {
  test('is posted with its id and minor units, then the loan list refreshes', async () => {
    api.farmLoansCreate.mockResolvedValue({ data: { id: 'l9' } })
    const { invalidate, wrapper } = client()
    const { result } = renderHook(() => hooks.useCreateLoanProduct(), { wrapper })
    const body = {
      id: 'l9',
      code: 'WOMEN',
      name_en: 'Women group loan',
      name_sw: 'Mkopo wa kikundi',
      description_en: null,
      description_sw: null,
      indicative_min_minor: 50000000,
      indicative_max_minor: 300000000,
      currency_id: 'c1',
    }

    await result.current.mutateAsync(body)

    expect(api.farmLoansCreate).toHaveBeenCalledWith('w1', body)
    expect(keys(invalidate)).toContainEqual(queryKeys.loanProducts())
  })
})
