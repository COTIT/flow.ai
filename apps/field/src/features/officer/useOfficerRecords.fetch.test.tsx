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

// The farm detail is not ported yet (its screen is gated); the cycle detail reads the Genesy API.
vi.mock('@/lib/supabase', () => ({ supabase: { from } }))

const api = {
  cycle: vi.fn(),
  plot: vi.fn(),
  harvests: vi.fn(),
  crops: vi.fn(),
}
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsCropCyclesGet: (...args: unknown[]) => api.cycle(...args),
  farmRecordsPlotsGet: (...args: unknown[]) => api.plot(...args),
  farmRecordsHarvestReportsList: (...args: unknown[]) => api.harvests(...args),
}))
vi.mock('@genesy/api-client/endpoints/farm-crops', () => ({
  farmCropsList: (...args: unknown[]) => api.crops(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const provenance = (verification = 'unverified') => ({
  source: 'farmer_reported',
  verification,
  confidence: 'medium',
  captured_at: '2026-09-01T00:00:00Z',
  captured_by: null,
  evidence_ref: null,
  verified_at: null,
  verified_by: null,
})
const notFound = { response: { status: 404, data: { message: 'The requested resource could not be found.' } } }

const { fetchFarmDetail, fetchCycleDetail, useFarmDetail } = await import('@/features/officer/useOfficerRecords')

const id = '11111111-1111-4111-8111-111111111111'

beforeEach(() => {
  responses.clear()
  from.mockClear()
  for (const fn of Object.values(api)) fn.mockReset()
  api.plot.mockResolvedValue({ data: { id: 'p1', label: 'Kipande cha juu' } })
  api.harvests.mockResolvedValue({ data: [] })
  api.crops.mockResolvedValue({ data: [{ id: 'crop', name_en: 'Maize', name_sw: 'Mahindi' }] })
})

describe('officer farm and cycle reads', () => {
  test('rejects malformed IDs before querying', async () => {
    expect(await fetchFarmDetail('bad')).toBeNull()
    expect(await fetchCycleDetail('bad')).toBeNull()
    expect(from).not.toHaveBeenCalled()
    expect(api.cycle).not.toHaveBeenCalled()
  })

  test('returns null for invisible rows and throws read errors', async () => {
    responses.set('farm', { data: null, error: null })
    api.cycle.mockRejectedValue(notFound)
    expect(await fetchFarmDetail(id)).toBeNull()
    expect(await fetchCycleDetail(id)).toBeNull()

    responses.set('farm', { data: null, error: { message: 'farm read failed' } })
    await expect(fetchFarmDetail(id)).rejects.toThrow('farm read failed')
    const failed = { response: { status: 500, data: { message: 'cycle read failed' } } }
    api.cycle.mockRejectedValue(failed)
    await expect(fetchCycleDetail(id)).rejects.toBe(failed)
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

    expect(api.cycle).not.toHaveBeenCalled()
  })

  // The cycle detail joins four reads on the client: the cycle, its plot's
  // label, its harvest series and the crop names (no single API call has all).
  test('joins the cycle, plot label, crop names and harvest series', async () => {
    api.cycle.mockResolvedValue({
      data: {
        id, village_id: 'v1', crop_id: 'crop', plot_id: 'p1', season_label: null, status: 'growing',
        area_ha: '1.6000', tree_count: null, unit_count: null,
        planted_on: null, harvest_start: null, harvest_end: null, provenance: provenance('verified'),
      },
    })
    api.harvests.mockResolvedValue({
      data: [
        { id: 'old', kind: 'expected', quantity_kg: '3200.00', is_current: false, reported_for: '2026-09-02', provenance: provenance() },
        { id: 'new', kind: 'expected', quantity_kg: '4100.00', is_current: true, reported_for: null, provenance: provenance() },
        { id: 'older', kind: 'expected', quantity_kg: '1.00', is_current: false, reported_for: '2026-09-01', provenance: provenance() },
        { id: 'oldest', kind: 'expected', quantity_kg: '1.00', is_current: false, reported_for: null, provenance: provenance() },
        { id: 'dated-before-null', kind: 'expected', quantity_kg: '1.00', is_current: false, reported_for: '2026-08-01', provenance: provenance() },
        { id: 'null-after-date', kind: 'expected', quantity_kg: '1.00', is_current: false, reported_for: null, provenance: provenance() },
      ],
    })

    const cycle = await fetchCycleDetail(id)

    expect(api.cycle).toHaveBeenCalledWith('w1', id)
    expect(api.plot).toHaveBeenCalledWith('w1', 'p1')
    expect(api.harvests).toHaveBeenCalledWith('w1', id, { page_size: 100 })
    expect(cycle).toMatchObject({
      crop: { name_en: 'Maize', name_sw: 'Mahindi' },
      plot_label: 'Kipande cha juu',
      area_ha: 1.6,
      verification: 'verified',
      source: 'farmer_reported',
      captured_at: '2026-09-01T00:00:00Z',
    })
    expect(cycle?.harvests[0]).toMatchObject({ id: 'new', quantity_kg: 4100, verification: 'unverified', confidence: 'medium' })
    expect(cycle?.harvests.map((item) => item.id)).toEqual(['new', 'old', 'older', 'dated-before-null', 'oldest', 'null-after-date'])
  })

  test('a cycle whose crop is not listed has no name, and a counted crop keeps its count', async () => {
    api.crops.mockResolvedValue({ data: [] })
    api.cycle.mockResolvedValue({
      data: {
        id, village_id: 'v1', crop_id: 'gone', plot_id: 'p1', season_label: null, status: 'planned',
        area_ha: null, tree_count: 40, unit_count: null,
        planted_on: null, harvest_start: null, harvest_end: null, provenance: provenance(),
      },
    })
    const cycle = await fetchCycleDetail(id)
    expect(cycle).toMatchObject({ crop: null, area_ha: null, tree_count: 40, harvests: [] })
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
