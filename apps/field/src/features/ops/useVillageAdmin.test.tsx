import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = { farmVillagesList: vi.fn(), farmVillagesCreate: vi.fn(), farmVillagesCapacityCreate: vi.fn() }
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)
vi.mock('@genesy/api-client/endpoints/farm-villages', () => ({
  farmVillagesList: forward('farmVillagesList'),
  farmVillagesCreate: forward('farmVillagesCreate'),
  farmVillagesCapacityCreate: forward('farmVillagesCapacityCreate'),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/useOpsReference')
const { queryKeys } = await import('@/lib/queryKeys')

const page = (data: unknown[], current = 1, last = 1) => ({
  data,
  meta: { current_page: current, last_page: last, per_page: 100, from: 1, to: data.length, total: data.length },
})
const capacity = { capacity_kw: '500.00', basis: 'planned', simultaneity_factor: '0.600', source_note: null, effective_from: '2026-01-15' }

function client() {
  const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
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

describe('villages with their current capacity', () => {
  test('reads every page and flattens the capacity', async () => {
    api.farmVillagesList
      .mockResolvedValueOnce(page([{ id: 'v1', name: 'Ilundo', code: 'ILU', capacity }], 1, 2))
      .mockResolvedValueOnce(page([{ id: 'v2', name: 'Mgama', code: 'MGA', capacity: null }], 2, 2))

    const rows = await hooks.fetchVillageCapacity()

    expect(api.farmVillagesList).toHaveBeenLastCalledWith('w1', { page: 2, page_size: 100 })
    expect(rows[0]).toEqual({
      id: 'v1', name: 'Ilundo', code: 'ILU', capacity_kw: 500, basis: 'planned',
      simultaneity_factor: '0.600', effective_from: '2026-01-15', source_note: null,
    })
    // Absent is not zero: no capacity row (or not the caller's village) reads as absent.
    expect(rows[1]).toMatchObject({ capacity_kw: null, basis: null, simultaneity_factor: null, effective_from: null })
  })
})

describe('creating a village', () => {
  test('sends the village with its first capacity, blanks as null', async () => {
    api.farmVillagesCreate.mockResolvedValue({ data: { id: 'v9' } })
    const { invalidate, wrapper } = client()
    const { result } = renderHook(() => hooks.useCreateVillage(), { wrapper })

    await result.current.mutateAsync({
      id: 'v9', project_id: 'w1', name: 'Mlowa', code: 'MLW', latitude: '', longitude: '35.62',
      capacity: { capacity_kw: '250', basis: 'planned', simultaneity_factor: '0.65', source_note: '', effective_from: '' },
    })

    expect(api.farmVillagesCreate).toHaveBeenCalledWith('w1', {
      id: 'v9', name: 'Mlowa', code: 'MLW', latitude: null, longitude: '35.62',
      capacity: { capacity_kw: '250', basis: 'planned', simultaneity_factor: '0.65', source_note: null, effective_from: null },
    })
    expect(keys(invalidate)).toEqual(
      expect.arrayContaining([queryKeys.villageCapacity(), queryKeys.villages(), ['tower']]),
    )
  })
})

describe('setting a capacity', () => {
  test('posts a new current capacity for that village', async () => {
    api.farmVillagesCapacityCreate.mockResolvedValue({ data: capacity })
    const { invalidate, wrapper } = client()
    const { result } = renderHook(() => hooks.useSetVillageCapacity(), { wrapper })

    await result.current.mutateAsync({
      villageId: 'v1',
      capacity: { capacity_kw: '600', basis: 'nameplate', simultaneity_factor: '', source_note: 'grid survey', effective_from: '2026-11-01' },
    })

    expect(api.farmVillagesCapacityCreate).toHaveBeenCalledWith('w1', 'v1', {
      capacity_kw: '600', basis: 'nameplate', simultaneity_factor: null, source_note: 'grid survey', effective_from: '2026-11-01',
    })
    expect(keys(invalidate)).toEqual(expect.arrayContaining([queryKeys.villageCapacity(), ['tower']]))
  })
})
