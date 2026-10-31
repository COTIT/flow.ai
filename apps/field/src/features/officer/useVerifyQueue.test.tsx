import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { describe, expect, test, vi } from 'vitest'

const responses = new Map<string, { data: unknown[] | null; error: { message: string } | null }>()
const rpc = vi.fn()
const from = vi.fn((table: string) => {
  const response = () => responses.get(table) ?? { data: [], error: null }
  const chain: Record<string, unknown> = {}
  for (const method of ['select', 'in', 'is']) chain[method] = vi.fn(() => chain)
  chain.then = (resolve: (value: unknown) => unknown, reject?: (reason: unknown) => unknown) =>
    Promise.resolve(response()).then(resolve, reject)
  return chain
})

vi.mock('@/lib/supabase', () => ({ supabase: { from, rpc } }))

const { fetchVerifyQueue, useVerifyQueue, useVerifyFromQueue } = await import('@/features/officer/useVerifyQueue')

const id = '11111111-1111-4111-8111-111111111111'

function seed() {
  responses.clear()
  const provenance = {
    village_id: 'v1', source: 'farmer_reported', verification: 'unverified', confidence: null,
    captured_at: '2026-09-20T00:00:00Z',
  }
  responses.set('person', { error: null, data: [{ id, given_name: 'A', family_name: 'B', ...provenance }] })
  responses.set('household', { error: null, data: [{ id: 'hh1', label: 'Kaya', captured_by: 'officer-1', ...provenance }] })
  responses.set('farm', { error: null, data: [{ id: 'f1', label: 'Farm', ...provenance }] })
  responses.set('plot', { error: null, data: [{ id: 'p1', farm_id: 'f1', label: 'Plot', ...provenance }] })
  responses.set('crop_cycle', { error: null, data: [{ id: 'c1', season_label: 'Season', crop: { name_en: 'Maize', name_sw: 'Mahindi' }, ...provenance }] })
  responses.set('harvest_report', { error: null, data: [{ id: 'h1', crop_cycle_id: 'c1', kind: 'expected', quantity_kg: 10, ...provenance }] })
}

describe('verify queue data', () => {
  test('loads all six record types and parent IDs', async () => {
    seed()
    const rows = await fetchVerifyQueue()
    expect(rows.map((row) => row.table).sort()).toEqual([
      'crop_cycle', 'farm', 'harvest_report', 'household', 'person', 'plot',
    ])
    expect(rows.find((row) => row.table === 'plot')?.farm_id).toBe('f1')
    expect(rows.find((row) => row.table === 'harvest_report')?.crop_cycle_id).toBe('c1')
  })

  // Households are verifiable too (app_verify accepts them), and a household
  // carries who registered it: the officer who did may not verify it
  // (20260929090002_household_four_eyes), so the screen needs to know.
  test('a household carries its label and who registered it', async () => {
    seed()
    const rows = await fetchVerifyQueue()
    expect(rows.find((row) => row.table === 'household')).toMatchObject({
      id: 'hh1',
      label: 'Kaya',
      captured_by: 'officer-1',
    })
    expect(from).toHaveBeenCalledWith('household')
  })

  test('a failed read rejects instead of returning an empty queue', async () => {
    seed()
    responses.set('plot', { data: [], error: { message: 'plot read failed' } })
    await expect(fetchVerifyQueue()).rejects.toThrow('plot read failed')
  })

  test('handles each missing query result as an empty list', async () => {
    for (const table of ['person', 'household', 'farm', 'plot', 'crop_cycle', 'harvest_report']) {
      seed()
      responses.set(table, { data: null, error: null })
      await expect(fetchVerifyQueue()).resolves.toBeDefined()
    }
  })

  test('hook localises cycle labels after fetching', async () => {
    seed()
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result } = renderHook(() => useVerifyQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data).toHaveLength(6))
    expect(result.current.data?.find((row) => row.table === 'crop_cycle')?.label).toContain('Maize')
  })

  test('uses an empty season label when the cycle has none', async () => {
    seed()
    responses.set('crop_cycle', { error: null, data: [{ id: 'c1', season_label: null, crop: null, village_id: 'v1', source: 'farmer_reported', verification: 'unverified', confidence: null, captured_at: '2026-09-20T00:00:00Z' }] })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result } = renderHook(() => useVerifyQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data).toHaveLength(6))
    expect(result.current.data?.find((row) => row.table === 'crop_cycle')?.label).toBe('')
  })

  test('verify mutation calls RPC and invalidates on success', async () => {
    rpc.mockResolvedValue({ error: null })
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
    expect(rpc).toHaveBeenCalledWith('app_verify', { p_table: 'person', p_id: id })
    expect(client.invalidateQueries).toHaveBeenCalled()
  })

  test('verify mutation exposes server failures', async () => {
    rpc.mockResolvedValue({ error: { message: 'stale record' } })
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const { result } = renderHook(() => useVerifyFromQueue(), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await expect(result.current.mutateAsync({ table: 'farm', id: 'f1' })).rejects.toThrow('stale record')
  })
})
