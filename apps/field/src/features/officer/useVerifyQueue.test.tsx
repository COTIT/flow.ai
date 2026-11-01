import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

const list = {
  persons: vi.fn(),
  households: vi.fn(),
  farms: vi.fn(),
  plots: vi.fn(),
  cycles: vi.fn(),
  harvests: vi.fn(),
}

const verify = {
  persons: vi.fn(),
  households: vi.fn(),
  farms: vi.fn(),
  plots: vi.fn(),
  cycles: vi.fn(),
  harvests: vi.fn(),
}
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsPersonsList: (...args: unknown[]) => list.persons(...args),
  farmRecordsHouseholdsList: (...args: unknown[]) => list.households(...args),
  farmRecordsFarmsList: (...args: unknown[]) => list.farms(...args),
  farmRecordsPlotsList: (...args: unknown[]) => list.plots(...args),
  farmRecordsCropCyclesList: (...args: unknown[]) => list.cycles(...args),
  farmRecordsHarvestReportsAll: (...args: unknown[]) => list.harvests(...args),
  farmRecordsPersonsVerify: (...args: unknown[]) => verify.persons(...args),
  farmRecordsHouseholdsVerify: (...args: unknown[]) => verify.households(...args),
  farmRecordsFarmsVerify: (...args: unknown[]) => verify.farms(...args),
  farmRecordsPlotsVerify: (...args: unknown[]) => verify.plots(...args),
  farmRecordsCropCyclesVerify: (...args: unknown[]) => verify.cycles(...args),
  farmRecordsHarvestReportsVerify: (...args: unknown[]) => verify.harvests(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchVerifyQueue, useVerifyQueue, useVerifyFromQueue } = await import('@/features/officer/useVerifyQueue')
const { default: i18n } = await import('@/i18n')

const id = '11111111-1111-4111-8111-111111111111'

const provenance = (captured_at: string, captured_by: { id: string; name: string } | null = null) => ({
  source: 'farmer_reported', verification: 'unverified', confidence: null, captured_at, captured_by,
  verified_by: null, verified_at: null, evidence_ref: null,
})
const page = (data: unknown[]) => ({ data, meta: { current_page: 1, last_page: 1, per_page: 100, total: data.length, from: null, to: null } })

function seed() {
  list.persons.mockResolvedValue(page([{ id, given_name: 'A', family_name: 'B', village_id: 'v1', provenance: provenance('2026-09-20T00:00:00Z') }]))
  list.households.mockResolvedValue(page([{ id: 'hh1', label: 'Kaya', village_id: 'v1', provenance: provenance('2026-09-22T00:00:00Z', { id: 'officer-1', name: 'Officer' }) }]))
  list.farms.mockResolvedValue(page([{ id: 'f1', label: 'Farm', village_id: 'v1', provenance: provenance('2026-09-19T00:00:00Z') }]))
  list.plots.mockResolvedValue(page([{ id: 'p1', farm_id: 'f1', label: 'Plot', village_id: 'v1', provenance: provenance('2026-09-21T00:00:00Z') }]))
  list.cycles.mockResolvedValue(page([
    { id: 'c1', season_label: 'Masika', crop: maize, village_id: 'v1', provenance: provenance('2026-09-18T00:00:00Z') },
    { id: 'c2', season_label: null, crop: maize, village_id: 'v1', provenance: provenance('2026-09-17T00:00:00Z') },
  ]))
  list.harvests.mockResolvedValue(page([
    { id: 'h1', crop_cycle_id: 'c1', kind: 'expected', quantity_kg: '4100.00', is_current: true, village_id: 'v1', provenance: provenance('2026-09-16T00:00:00Z') },
  ]))
}
const maize = { id: 'crop-1', code: 'maize', name_en: 'Maize', name_sw: 'Mahindi', measured_by: 'area' }

describe('verify queue data', () => {
  // ?verification=unverified,pending on each list, every page: the API's answer, nothing filtered here.
  test('lists the outstanding records of six kinds, newest first, with their parents', async () => {
    seed()
    const rows = await fetchVerifyQueue()
    for (const fn of Object.values(list)) {
      expect(fn).toHaveBeenCalledWith('w1', { verification: 'unverified,pending', page: 1, page_size: 100 })
    }
    expect(rows.map((row) => `${row.table}:${row.label}`)).toEqual([
      'household:Kaya', 'plot:Plot', 'person:A B', 'farm:Farm', 'crop_cycle:Masika', 'crop_cycle:', 'harvest_report:4100.00 kg',
    ])
    expect(rows.find((row) => row.table === 'plot')?.farm_id).toBe('f1')
    // The workspace-wide harvest list, role-scoped by the API; a figure opens its cycle.
    expect(rows.find((row) => row.table === 'harvest_report')).toMatchObject({ id: 'h1', crop_cycle_id: 'c1', harvest_kind: 'expected' })
    expect(rows[2]).toEqual({
      table: 'person', id, label: 'A B', village_id: 'v1', source: 'farmer_reported', verification: 'unverified',
      confidence: null, captured_at: '2026-09-20T00:00:00Z',
    })
  })

  // The officer who registered a household may not verify it; the screen needs to know who did.
  test('a household carries who registered it', async () => {
    seed()
    const rows = await fetchVerifyQueue()
    expect(rows.find((row) => row.table === 'household')).toMatchObject({ id: 'hh1', label: 'Kaya', captured_by: 'officer-1' })
    list.households.mockResolvedValue(page([{ id: 'hh2', label: 'Kaya', village_id: 'v1', provenance: provenance('2026-09-22T00:00:00Z') }]))
    expect((await fetchVerifyQueue()).find((row) => row.table === 'household')?.captured_by).toBeNull()
  })

  test('a failed read rejects instead of returning an empty queue', async () => {
    seed()
    const failed = { response: { status: 500, data: { message: 'plot read failed' } } }
    list.plots.mockRejectedValue(failed)
    await expect(fetchVerifyQueue()).rejects.toBe(failed)
  })

  test('the hook serves the queue', async () => {
    seed()
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result } = renderHook(() => useVerifyQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data).toHaveLength(7))
  })

  // Completed at render, in the language showing then: one cache entry serves both languages.
  test("labels a cycle with its embedded crop and a figure with its kind, in the reader's language", async () => {
    seed()
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result, rerender } = renderHook(() => useVerifyQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data).toHaveLength(7))
    const labels = () => result.current.data!.filter((row) => row.table !== 'person' && row.table !== 'household' && row.table !== 'farm' && row.table !== 'plot').map((row) => row.label)
    expect(labels()).toEqual(['Maize · Masika', 'Maize', 'Expected · 4100.00 kg'])
    await i18n.changeLanguage('sw')
    rerender()
    expect(labels()[0]).toBe('Mahindi · Masika')
    await i18n.changeLanguage('en')
  })

  test('verify mutation calls the API and invalidates on success', async () => {
    verify.persons.mockResolvedValue({ data: {} })
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    client.invalidateQueries = vi.fn((filters?: { predicate?: (query: { queryKey: readonly unknown[] }) => boolean }) => {
      filters?.predicate?.({ queryKey: ['tower'] })
      filters?.predicate?.({ queryKey: ['other'] })
      return Promise.resolve()
    }) as typeof client.invalidateQueries
    const { result } = renderHook(() => useVerifyFromQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await result.current.mutateAsync({ table: 'person', id })
    expect(verify.persons).toHaveBeenCalledWith('w1', id)
    // The cycle detail verifies a cycle and its harvest figures too: its badges must follow.
    expect(client.invalidateQueries).toHaveBeenCalledWith({ queryKey: ['cycle'] })
  })

  test('each record type verifies through its own route', async () => {
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const { result } = renderHook(() => useVerifyFromQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    const pairs = [
      ['household', verify.households],
      ['farm', verify.farms],
      ['plot', verify.plots],
      ['crop_cycle', verify.cycles],
      ['harvest_report', verify.harvests],
    ] as const
    for (const [table, fn] of pairs) {
      fn.mockResolvedValue({ data: {} })
      await result.current.mutateAsync({ table, id })
      expect(fn).toHaveBeenCalledWith('w1', id)
    }
  })

  test('verify mutation exposes server failures', async () => {
    verify.farms.mockRejectedValue(new Error('stale record'))
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const { result } = renderHook(() => useVerifyFromQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await expect(result.current.mutateAsync({ table: 'farm', id: 'f1' })).rejects.toThrow('stale record')
  })
})
