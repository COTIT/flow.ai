import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = { person: vi.fn(), holdings: vi.fn(), verifyPlot: vi.fn(), verifyFarm: vi.fn(), verifyPerson: vi.fn() }
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsPersonsGet: (...args: unknown[]) => api.person(...args),
  farmRecordsPersonsHoldings: (...args: unknown[]) => api.holdings(...args),
  farmRecordsPersonsVerify: (...args: unknown[]) => api.verifyPerson(...args),
  farmRecordsHouseholdsVerify: vi.fn(),
  farmRecordsFarmsVerify: (...args: unknown[]) => api.verifyFarm(...args),
  farmRecordsPlotsVerify: (...args: unknown[]) => api.verifyPlot(...args),
  farmRecordsCropCyclesVerify: vi.fn(),
  farmRecordsHarvestReportsVerify: vi.fn(),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchPersonDetail, usePersonDetail, useVerify } = await import('@/features/officer/usePersonDetail')
const { default: i18n } = await import('@/i18n')

const id = '11111111-1111-4111-8111-111111111111'
const provenance = {
  source: 'field_verified', captured_at: '2026-09-01T00:00:00Z', captured_by: { id: 'officer-1', name: 'Officer' },
  verification: 'unverified', verified_by: null, verified_at: null, confidence: 'high', evidence_ref: null,
}
const flat = {
  source: 'field_verified', captured_at: '2026-09-01T00:00:00Z', captured_by: 'officer-1',
  verification: 'unverified', verified_by: null, verified_at: null, confidence: 'high',
}
const person = { id, given_name: 'Neema', family_name: 'Mwakalinga', phone: '+255712345678', village_id: 'v1', provenance }
const cycle = (crop: unknown) => ({
  id: 'c1', crop_id: 'crop', plot_id: 'p1', village_id: 'v1', season_label: 'Masika', status: 'growing',
  area_ha: '1.6000', tree_count: null, unit_count: null, planted_on: null, harvest_start: '2026-09-01', harvest_end: null,
  provenance, crop, harvests: [{
    id: 'hr1', kind: 'expected', quantity_kg: '4100.00', is_current: true, reported_for: null,
    provenance: { ...provenance, verified_by: { id: 'officer-2', name: 'Second' }, verified_at: '2026-09-02T00:00:00Z', captured_by: null },
  }],
})
const holdings = (crop: unknown = { id: 'crop', code: 'maize', name_en: 'Maize', name_sw: 'Mahindi' }) => ({ data: {
  households: [{ id: 'h1', label: 'Mwakalinga household', members: [], provenance, people: [
    { person_id: id, given_name: 'Neema', family_name: 'Mwakalinga', is_head: true },
    { person_id: 'p2', given_name: 'Juma', family_name: 'Mwakalinga', is_head: false },
  ] }],
  farms: [{ id: 'f1', label: 'Riverside farm', latitude: '-7.710000', longitude: null, provenance, plots: [
    { id: 'p1', label: 'Riverside', area_ha: '1.8000', latitude: null, longitude: null, provenance, cycles: [cycle(crop)] },
  ] }],
} })
const notFound = { response: { status: 404, data: { message: 'The requested resource could not be found.' } } }

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
  void i18n.changeLanguage('en')
  api.person.mockResolvedValue({ data: person })
  api.holdings.mockResolvedValue(holdings())
})

describe('fetchPersonDetail', () => {
  test('returns null for malformed or invisible IDs', async () => {
    expect(await fetchPersonDetail('bad')).toBeNull()
    expect(api.person).not.toHaveBeenCalled()
    api.person.mockRejectedValue(notFound)
    expect(await fetchPersonDetail(id)).toBeNull()
  })

  test('surfaces any other read error as it came', async () => {
    const failed = { response: { status: 500, data: { message: 'person failed' } } }
    api.person.mockRejectedValue(failed)
    await expect(fetchPersonDetail(id)).rejects.toBe(failed)
    api.person.mockResolvedValue({ data: person })
    api.holdings.mockRejectedValue(failed)
    await expect(fetchPersonDetail(id)).rejects.toBe(failed)
  })

  // One person read plus one holdings read; household people come with their names.
  test('maps the person and their holdings', async () => {
    const detail = await fetchPersonDetail(id)
    expect(api.holdings).toHaveBeenCalledWith('w1', id)
    expect(detail?.person).toEqual({ id, given_name: 'Neema', family_name: 'Mwakalinga', phone: '+255712345678', village_id: 'v1', ...flat })
    expect(detail?.households).toEqual([{ id: 'h1', label: 'Mwakalinga household', ...flat, members: [
      { id, given_name: 'Neema', family_name: 'Mwakalinga' },
      { id: 'p2', given_name: 'Juma', family_name: 'Mwakalinga' },
    ] }])
    const farm = detail!.farms[0]
    expect(farm).toMatchObject({ id: 'f1', label: 'Riverside farm', latitude: -7.71, longitude: null, ...flat })
    expect(farm.plots[0]).toMatchObject({ id: 'p1', label: 'Riverside', area_ha: 1.8, latitude: null, longitude: null })
    expect(farm.plots[0].cycles[0]).toMatchObject({
      id: 'c1', crop_id: 'crop', season_label: 'Masika', status: 'growing', area_ha: 1.6, harvest_start: '2026-09-01',
      crop: { name_en: 'Maize', name_sw: 'Mahindi' },
    })
    expect(farm.plots[0].cycles[0].harvests).toEqual([{
      id: 'hr1', kind: 'expected', quantity_kg: 4100, is_current: true, reported_for: null,
      ...flat, captured_by: null, verified_by: 'officer-2', verified_at: '2026-09-02T00:00:00Z',
    }])
  })

  test('hook maps translated and fallback crop names and retains a null result', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const { result, rerender } = renderHook(({ personId }) => usePersonDetail(personId), {
      initialProps: { personId: id },
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await waitFor(() => expect(result.current.data?.farms[0].plots[0].cycles[0].crop_name).toBe('Maize'))

    await i18n.changeLanguage('sw')
    rerender({ personId: id })
    await waitFor(() => expect(result.current.data?.farms[0].plots[0].cycles[0].crop_name).toBe('Mahindi'))

    api.holdings.mockResolvedValue(holdings(null))
    client.clear()
    rerender({ personId: '22222222-2222-4222-8222-222222222222' })
    await waitFor(() => expect(result.current.data?.farms[0].plots[0].cycles[0].crop_name).toBe('crop'))

    api.person.mockRejectedValue(notFound)
    client.clear()
    rerender({ personId: '33333333-3333-4333-8333-333333333333' })
    await waitFor(() => expect(result.current.data).toBeNull())
  })

  test('verification posts to the record kind and invalidates all related keys', async () => {
    api.verifyPlot.mockResolvedValue({ data: {} })
    api.verifyFarm.mockResolvedValue({ data: {} })
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    client.invalidateQueries = vi.fn().mockResolvedValue(undefined)
    const { result } = renderHook(() => useVerify(id, 'v1'), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })

    await result.current.mutateAsync({ table: 'plot', id: 'plot-1' })
    expect(api.verifyPlot).toHaveBeenCalledWith('w1', 'plot-1')
    expect(client.invalidateQueries).toHaveBeenCalledTimes(6)

    const noVillage = renderHook(() => useVerify(id, undefined), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await noVillage.result.current.mutateAsync({ table: 'farm', id: 'farm-1' })
    expect(api.verifyFarm).toHaveBeenCalledWith('w1', 'farm-1')
    expect(client.invalidateQueries).toHaveBeenCalledTimes(7)
  })

  test('verification keeps the API refusal as it came', async () => {
    const refused = { response: { status: 422, data: { message: 'record is not awaiting verification' } } }
    api.verifyPerson.mockRejectedValue(refused)
    const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const { result } = renderHook(() => useVerify(id, 'v1'), {
      wrapper: ({ children }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
    })
    await expect(result.current.mutateAsync({ table: 'person', id })).rejects.toBe(refused)
  })
})
