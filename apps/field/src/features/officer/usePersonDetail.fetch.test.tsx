import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const responses = new Map<string, { data: unknown; error: { message: string } | null }>()
const rpc = vi.fn()
const from = vi.fn((table: string) => {
  const response = () => responses.get(table) ?? { data: [], error: null }
  const chain: Record<string, unknown> = {}
  for (const method of ['select', 'eq', 'is', 'in', 'or']) chain[method] = vi.fn(() => chain)
  chain.maybeSingle = vi.fn(() => Promise.resolve(response()))
  chain.then = (resolve: (value: unknown) => unknown, reject?: (reason: unknown) => unknown) =>
    Promise.resolve(response()).then(resolve, reject)
  return chain
})

vi.mock('@/lib/supabase', () => ({ supabase: { from, rpc } }))

const { fetchPersonDetail, usePersonDetail, useVerify } = await import('@/features/officer/usePersonDetail')
const { default: i18n } = await import('@/i18n')

const id = '11111111-1111-4111-8111-111111111111'

beforeEach(() => {
  responses.clear()
  from.mockClear()
  rpc.mockReset()
  void i18n.changeLanguage('en')
  responses.set('household_member', { data: [], error: null })
  responses.set('farm_manager', { data: [], error: null })
  responses.set('household', { data: [], error: null })
  responses.set('farm', { data: [], error: null })
})

describe('fetchPersonDetail', () => {
  test('returns null for malformed or invisible IDs', async () => {
    expect(await fetchPersonDetail('bad')).toBeNull()
    responses.set('person', { data: null, error: null })
    expect(await fetchPersonDetail(id)).toBeNull()
  })

  test('surfaces every graph read error', async () => {
    responses.set('person', { data: null, error: { message: 'person failed' } })
    await expect(fetchPersonDetail(id)).rejects.toThrow('person failed')

    responses.set('person', { data: { id, village_id: 'v1' }, error: null })
    responses.set('household_member', { data: [], error: { message: 'membership failed' } })
    await expect(fetchPersonDetail(id)).rejects.toThrow('membership failed')

    responses.set('household_member', { data: [], error: null })
    responses.set('farm_manager', { data: [], error: { message: 'manager failed' } })
    await expect(fetchPersonDetail(id)).rejects.toThrow('manager failed')

    responses.set('farm_manager', { data: [], error: null })
    responses.set('household_member', { data: [{ household_id: 'h1' }], error: null })
    responses.set('household', { data: [], error: { message: 'household failed' } })
    await expect(fetchPersonDetail(id)).rejects.toThrow('household failed')

    responses.set('household', { data: [], error: null })
    responses.set('farm', { data: [], error: { message: 'farm failed' } })
    await expect(fetchPersonDetail(id)).rejects.toThrow('farm failed')
  })

  test('treats missing top-level and nested relationship arrays as empty', async () => {
    responses.set('person', { data: { id, village_id: 'v1', given_name: 'Neema', family_name: 'A' }, error: null })
    responses.set('household_member', { data: undefined, error: null })
    responses.set('farm_manager', { data: undefined, error: null })
    responses.set('farm', { data: undefined, error: null })
    const empty = await fetchPersonDetail(id)
    expect(empty?.households).toEqual([])
    expect(empty?.farms).toEqual([])

    responses.set('household_member', { data: [{ household_id: 'missing-household' }], error: null })
    responses.set('household', { data: undefined, error: null })
    responses.set('farm_manager', { data: [], error: null })
    const missingHouseholds = await fetchPersonDetail(id)
    expect(missingHouseholds?.households).toEqual([])

    responses.set('household_member', { data: [{ household_id: 'h1' }], error: null })
    responses.set('farm_manager', { data: [{ farm_id: 'f1' }], error: null })
    responses.set('household', { data: [{ id: 'h1' }], error: null })
    responses.set('farm', { data: [{ id: 'f0' }, { id: 'f1', plot: [{ id: 'p0' }, { id: 'p1', crop_cycle: [{ id: 'c1' }] }] }], error: null })
    const nested = await fetchPersonDetail(id)
    expect(nested?.households[0].members).toEqual([])
    expect(nested?.farms[0].plots).toEqual([])
    expect(nested?.farms[1].plots[0].cycles).toEqual([])
    expect(nested?.farms[1].plots[1].cycles[0].harvests).toEqual([])
  })

  test('maps memberships, null people, farms, plots, cycles, and harvests', async () => {
    responses.set('person', {
      data: { id, village_id: 'v1', given_name: 'Neema', family_name: 'A' }, error: null,
    })
    responses.set('household_member', { data: [{ household_id: 'h1' }], error: null })
    responses.set('farm_manager', { data: [{ farm_id: 'f1' }], error: null })
    responses.set('household', {
      data: [{ id: 'h1', household_member: [{ person: null }, { person: { id, given_name: 'Neema', family_name: 'A' } }] }], error: null,
    })
    responses.set('farm', {
      data: [{
        id: 'f1',
        plot: [{
          id: 'p1',
          crop_cycle: [{ id: 'c1', crop: { name_en: 'Maize', name_sw: 'Mahindi' }, harvest_report: [{ id: 'h1' }] }],
        }],
      }],
      error: null,
    })
    const detail = await fetchPersonDetail(id)
    expect(detail?.households[0].members).toEqual([{ id, given_name: 'Neema', family_name: 'A' }])
    expect(detail?.farms[0].plots[0].cycles[0].harvests).toEqual([{ id: 'h1' }])
  })

  test('hook maps translated, missing, and fallback crop names and retains a null result', async () => {
    responses.set('person', { data: { id, village_id: 'v1', given_name: 'Neema', family_name: 'A' }, error: null })
    responses.set('farm', { data: [{ id: 'f1', plot: [{ id: 'p1', crop_cycle: [
      { crop_id: 'crop', crop: { name_en: 'Maize', name_sw: 'Mahindi' }, harvest_report: [] },
      { crop_id: 'fallback', crop: null, harvest_report: [] },
    ] }] }], error: null })
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result, rerender } = renderHook(({ personId }) => usePersonDetail(personId), {
      initialProps: { personId: id },
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data?.farms[0].plots[0].cycles[0].crop_name).toBe('Maize'))
    expect(result.current.data?.farms[0].plots[0].cycles[1].crop_name).toBe('fallback')

    await i18n.changeLanguage('sw')
    rerender({ personId: id })
    await waitFor(() => expect(result.current.data?.farms[0].plots[0].cycles[0].crop_name).toBe('Mahindi'))

    responses.set('person', { data: null, error: null })
    client.clear()
    rerender({ personId: '33333333-3333-4333-8333-333333333333' })
    await waitFor(() => expect(result.current.data).toBeNull())
  })

  test('verification mutation stamps through RPC and invalidates all related keys', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    client.invalidateQueries = vi.fn().mockResolvedValue(undefined)
    const { result } = renderHook(() => useVerify(id, 'v1'), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })

    await result.current.mutateAsync({ table: 'plot', id: 'plot-1' })
    expect(rpc).toHaveBeenCalledWith('app_verify', { p_table: 'plot', p_id: 'plot-1' })
    expect(client.invalidateQueries).toHaveBeenCalledTimes(6)

    const noVillage = renderHook(() => useVerify(id, undefined), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await noVillage.result.current.mutateAsync({ table: 'farm', id: 'farm-1' })
    expect(client.invalidateQueries).toHaveBeenCalledTimes(7)
  })

  test('verification mutation keeps the exact server error', async () => {
    rpc.mockResolvedValue({ error: { message: 'stale verification state' } })
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const { result } = renderHook(() => useVerify(id, 'v1'), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await expect(result.current.mutateAsync({ table: 'person', id })).rejects.toThrow('stale verification state')
  })
})
