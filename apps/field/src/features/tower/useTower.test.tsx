import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmTowerProduction: vi.fn(),
  farmTowerDataQuality: vi.fn(),
  farmPueVillageEnergy: vi.fn(),
  farmPueVillagePipeline: vi.fn(),
  farmPueRequestsList: vi.fn(),
  farmMarketVillageMatches: vi.fn(),
  farmRecordsCropCyclesList: vi.fn(),
  farmRecordsHarvestReportsAll: vi.fn(),
  farmRecordsPlotsList: vi.fn(),
  farmRecordsFarmsList: vi.fn(),
  farmRecordsPersonsList: vi.fn(),
  farmCropsList: vi.fn(),
}
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)
vi.mock('@genesy/api-client/endpoints/farm-tower', () => ({
  farmTowerProduction: forward('farmTowerProduction'),
  farmTowerDataQuality: forward('farmTowerDataQuality'),
}))
vi.mock('@genesy/api-client/endpoints/farm-pue', () => ({
  farmPueVillageEnergy: forward('farmPueVillageEnergy'),
  farmPueVillagePipeline: forward('farmPueVillagePipeline'),
  farmPueRequestsList: forward('farmPueRequestsList'),
}))
vi.mock('@genesy/api-client/endpoints/farm-market', () => ({
  farmMarketVillageMatches: forward('farmMarketVillageMatches'),
}))
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsCropCyclesList: forward('farmRecordsCropCyclesList'),
  farmRecordsHarvestReportsAll: forward('farmRecordsHarvestReportsAll'),
  farmRecordsPlotsList: forward('farmRecordsPlotsList'),
  farmRecordsFarmsList: forward('farmRecordsFarmsList'),
  farmRecordsPersonsList: forward('farmRecordsPersonsList'),
}))
vi.mock('@genesy/api-client/endpoints/farm-crops', () => ({ farmCropsList: forward('farmCropsList') }))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/tower/useTower')
await import('@/i18n')

const V = 'v1'
const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})
const maize = { id: 'c1', code: 'MAIZE', name_en: 'Maize', name_sw: 'Mahindi', measured_by: 'area', is_active: true }
const prov = (verification = 'unverified') => ({
  source: 'farmer_reported', verification, confidence: null, evidence_ref: null,
  captured_at: '2026-09-01T00:00:00Z', captured_by: null, verified_at: null, verified_by: null,
})
const cycle = { id: 'cy1', crop_id: 'c1', crop: { name_en: 'Maize', name_sw: 'Mahindi' }, plot_id: 'pl1', village_id: V,
  season_label: '2026 long rains', area_ha: '1.6000', tree_count: null, unit_count: null,
  harvest_start: '2026-09-01', harvest_end: '2026-09-30', status: 'growing', provenance: prov('verified') }
const report = (kind: string, quantity_kg: string) => ({ id: `h-${kind}`, crop_cycle_id: 'cy1', kind, quantity_kg, is_current: true, village_id: V })

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
  api.farmCropsList.mockResolvedValue(page([maize]))
})

describe('the Tower reads the API figures as they are', () => {
  test('production: the API rows, named, with cycle_area_ha as the sum of cycle areas', async () => {
    api.farmTowerProduction.mockResolvedValue({ data: [{
      crop_id: 'c1', window_month: '2026-09-01', cycle_count: 2, plot_count: 1, cycle_area_ha: '1.6000',
      tree_count: null, unit_count: null, expected_kg: '3200.00', actual_kg: null, verified_cycles: 1,
    }] })
    const { result } = renderHook(() => hooks.useTowerProduction(V), { wrapper })

    await waitFor(() => expect(result.current.data?.[0]?.crop_name).toBe('Maize'))
    expect(api.farmTowerProduction).toHaveBeenCalledWith('w1', V)
    expect(result.current.data?.[0]).toMatchObject({
      cycle_area_ha: 1.6, expected_kg: 3200, actual_kg: null, cycle_count: 2, verified_cycles: 1,
    })
  })

  test('energy, quality and the pipeline come back as the API sent them', async () => {
    api.farmPueVillageEnergy.mockResolvedValue({ data: {
      village_id: V, capacity_kw: '500.00', capacity_basis: 'planned', simultaneity_factor: '0.600',
      prospective_kw_raw: '15.000', approved_kw_raw: '18.000', prospective_peak_kw: '9.000',
      approved_peak_kw: '10.800', headroom_kw: '489.200', approved_kwh_per_week: '828.000', approved_assets: 2,
    } })
    const quality = { village_id: V, persons: 6, persons_verified: 3, farms: 4, farms_with_gps: 3, cycles: 7, cycles_with_estimate: 5 }
    api.farmTowerDataQuality.mockResolvedValue({ data: quality })
    const money = { minor: 3860000000, decimals: 2, currency: 'TZS', currency_id: 'tzs' }
    api.farmPueVillagePipeline.mockResolvedValue({ data: [{ status: 'approved', request_count: 2, indicative_value: money, indicative_rent_per_day: null }] })

    const energy = renderHook(() => hooks.useTowerEnergy(V), { wrapper }).result
    const q = renderHook(() => hooks.useTowerQuality(V), { wrapper }).result
    const pipeline = renderHook(() => hooks.useTowerPipeline(V), { wrapper }).result

    await waitFor(() => expect(energy.current.data).toBeTruthy())
    expect(energy.current.data).toMatchObject({
      capacity_kw: 500, capacity_basis: 'planned', simultaneity_factor: 0.6, prospective_peak_kw: 9,
      approved_peak_kw: 10.8, headroom_kw: 489.2, prospective_kw_raw: 15, approved_kw_raw: 18, approved_kwh_per_week: 828,
    })
    await waitFor(() => expect(q.current.data).toEqual(quality))
    await waitFor(() => expect(pipeline.current.data?.[0]?.indicative_value).toEqual(money))
  })

  test('market: the village demand matches, named, with their opportunity', async () => {
    api.farmMarketVillageMatches.mockResolvedValue({ data: [{
      buyer_demand_id: 'd1', buyer: { id: 'b1', name: 'Iringa Grain' }, buyer_name: 'Iringa Grain', crop_id: 'c1',
      demand_kg: '9000.00', window_start: '2026-09-01', window_end: '2026-10-31', village: { id: V, name: 'Ilundo' },
      available_kg: '5600.00', committed_kg: '0.00', coverable_kg: '5600.00', coverage_pct: '62.2',
      opportunity: { id: 'o1', status: 'shared' },
    }] })
    const { result } = renderHook(() => hooks.useTowerMarket(V), { wrapper })

    await waitFor(() => expect(result.current.data?.matches[0]?.crop_name).toBe('Maize'))
    expect(result.current.data?.matches[0]).toMatchObject({
      buyer_demand_id: 'd1', village_id: V, buyer_name: 'Iringa Grain', demand_kg: 9000, available_kg: 5600,
      coverage_pct: 62.2, opportunity_id: 'o1', opportunity_status: 'shared',
    })
  })

  test('a refused read is an error, not an empty tile', async () => {
    api.farmTowerDataQuality.mockRejectedValue(new Error('Not found'))
    const { result } = renderHook(() => hooks.useTowerQuality(V), { wrapper })
    await waitFor(() => expect(result.current.error).toBeTruthy())
  })
})

describe('the rows behind each figure', () => {
  test('energy rows: the village requests with their stored estimate', async () => {
    api.farmPueRequestsList.mockResolvedValue(page([{
      id: 'r1', status: 'under_review', person: { id: 'p1', name: 'Baraka Mushi' },
      equipment: { id: 'e1', code: 'DRY', name_en: 'Dryer', name_sw: 'Kikaushio' },
      estimate: { est_power_kw: '12.000' },
    }]))
    const { result } = renderHook(() => hooks.useTowerEnergyRows(V), { wrapper })

    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(api.farmPueRequestsList).toHaveBeenCalledWith('w1', { village_id: V, page: 1, page_size: 100 })
    expect(result.current.data?.[0]).toMatchObject({
      id: 'r1', status: 'under_review', applicant: 'Baraka Mushi', person_id: 'p1', equipment_name: 'Dryer', est_power_kw: 12,
    })
  })

  test('production rows: each cycle with its plot, farmer and current figures', async () => {
    api.farmRecordsCropCyclesList.mockResolvedValue(page([cycle]))
    api.farmRecordsHarvestReportsAll.mockResolvedValue(page([report('expected', '3200.00')]))
    api.farmRecordsPlotsList.mockResolvedValue(page([{ id: 'pl1', label: 'Kipande A', farm_id: 'f1' }]))
    api.farmRecordsFarmsList.mockResolvedValue(page([{ id: 'f1', label: 'Shamba', managers: [{ person_id: 'p1', is_primary: true }] }]))
    api.farmRecordsPersonsList.mockResolvedValue(page([{ id: 'p1', given_name: 'Neema', family_name: 'Mushi', provenance: prov() }]))
    const { result } = renderHook(() => hooks.useTowerProductionRows(V), { wrapper })

    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(api.farmRecordsHarvestReportsAll).toHaveBeenCalledWith('w1', { village_id: V, current: true, page: 1, page_size: 100 })
    expect(result.current.data?.[0]).toMatchObject({
      id: 'cy1', crop_name: 'Maize', area_ha: 1.6, plot_label: 'Kipande A', farmer: 'Neema Mushi', person_id: 'p1',
      expected_kg: 3200, actual_kg: null, harvest_start: '2026-09-01',
    })
  })

  test('quality rows: persons by verification, farms by GPS, cycles by a current estimate', async () => {
    api.farmRecordsPersonsList.mockResolvedValue(page([
      { id: 'p1', given_name: 'Neema', family_name: 'Mushi', provenance: prov('verified') },
      { id: 'p2', given_name: 'Joseph', family_name: 'Mushi', provenance: prov('unverified') },
    ]))
    api.farmRecordsFarmsList.mockResolvedValue(page([{ id: 'f1', label: 'Shamba', latitude: '-7.1', longitude: '35.2' }]))
    api.farmRecordsCropCyclesList.mockResolvedValue(page([cycle]))
    api.farmRecordsHarvestReportsAll.mockResolvedValue(page([report('actual', '100.00')]))

    const persons = renderHook(() => hooks.useTowerQualityRows(V, 'persons'), { wrapper }).result
    const farms = renderHook(() => hooks.useTowerQualityRows(V, 'farms'), { wrapper }).result
    const cycles = renderHook(() => hooks.useTowerQualityRows(V, 'cycles'), { wrapper }).result

    await waitFor(() => expect(persons.current.data).toHaveLength(2))
    expect(persons.current.data?.map((r) => [r.label, r.detail, r.qualifies])).toEqual([
      ['Neema Mushi', 'verified', true],
      ['Joseph Mushi', 'unverified', false],
    ])
    await waitFor(() => expect(farms.current.data?.[0]).toMatchObject({ kind: 'farm', detail: '-7.1, 35.2', qualifies: true }))
    // An actual figure is not an estimate.
    await waitFor(() => expect(cycles.current.data?.[0]).toMatchObject({ kind: 'cycle', label: 'Maize', qualifies: false }))
  })
})
