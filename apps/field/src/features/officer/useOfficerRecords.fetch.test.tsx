import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const responses = new Map<string, { data: unknown; error: { message: string } | null }>()
const from = vi.fn((table: string) => {
  const response = () => responses.get(table) ?? { data: null, error: null }
  const chain: Record<string, unknown> = {}
  for (const method of ['select', 'eq', 'is', 'in', 'or', 'order']) {
    chain[method] = vi.fn(() => chain)
  }
  chain.maybeSingle = vi.fn(() => Promise.resolve(response()))
  chain.then = (resolve: (value: unknown) => unknown, reject?: (reason: unknown) => unknown) =>
    Promise.resolve(response()).then(resolve, reject)
  return chain
})

vi.mock('@/lib/supabase', () => ({ supabase: { from } }))

const { fetchFarmDetail, fetchCycleDetail, useFarmDetail } = await import('@/features/officer/useOfficerRecords')

const id = '11111111-1111-4111-8111-111111111111'

beforeEach(() => {
  responses.clear()
  from.mockClear()
})

describe('officer farm and cycle reads', () => {
  test('rejects malformed IDs before querying', async () => {
    expect(await fetchFarmDetail('bad')).toBeNull()
    expect(await fetchCycleDetail('bad')).toBeNull()
    expect(from).not.toHaveBeenCalled()
  })

  test('returns null for invisible rows and throws read errors', async () => {
    responses.set('farm', { data: null, error: null })
    responses.set('crop_cycle', { data: null, error: null })
    expect(await fetchFarmDetail(id)).toBeNull()
    expect(await fetchCycleDetail(id)).toBeNull()

    responses.set('farm', { data: null, error: { message: 'farm read failed' } })
    await expect(fetchFarmDetail(id)).rejects.toThrow('farm read failed')
    responses.set('crop_cycle', { data: null, error: { message: 'cycle read failed' } })
    await expect(fetchCycleDetail(id)).rejects.toThrow('cycle read failed')
  })

  test('maps farm plots and sorts cycle current harvest first', async () => {
    responses.set('farm', {
      error: null,
      data: {
        id,
        label: 'Farm',
        village_id: 'v1',
        plot: [{ id: 'p1', label: 'Plot', area_ha: 1, latitude: null, longitude: null }],
      },
    })
    const farm = await fetchFarmDetail(id)
    expect(farm?.plots).toHaveLength(1)
    responses.set('farm', { error: null, data: { id, label: 'Farm', village_id: 'v1' } })
    expect((await fetchFarmDetail(id))?.plots).toEqual([])

    responses.set('crop_cycle', {
      error: null,
      data: {
        id,
        village_id: 'v1',
        crop_id: 'crop',
        crop: { name_en: 'Maize', name_sw: 'Mahindi' },
        plot: null,
        harvest_report: [
          { id: 'old', is_current: false, reported_for: '2026-09-02' },
          { id: 'new', is_current: true, reported_for: null },
          { id: 'older', is_current: false, reported_for: '2026-09-01' },
          { id: 'oldest', is_current: false, reported_for: null },
          { id: 'dated-before-null', is_current: false, reported_for: '2026-08-01' },
          { id: 'null-after-date', is_current: false, reported_for: null },
        ],
      },
    })
    const cycle = await fetchCycleDetail(id)
    expect(cycle?.plot_label).toBeNull()
    expect(cycle?.harvests.map((item) => item.id)).toEqual(['new', 'old', 'older', 'dated-before-null', 'oldest', 'null-after-date'])
    responses.set('crop_cycle', { error: null, data: { id, village_id: 'v1', crop: null, plot: null } })
    expect((await fetchCycleDetail(id))?.harvests).toEqual([])
  })

  test('useFarmDetail exposes query loading and data', async () => {
    responses.set('farm', { error: null, data: { id, label: 'Farm', village_id: 'v1', plot: [] } })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result } = renderHook(() => useFarmDetail(id), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data?.label).toBe('Farm'))
  })
})
