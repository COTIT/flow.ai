import { beforeEach, expect, test, vi } from 'vitest'

const api = {
  farmRecordsHouseholdsList: vi.fn(),
  farmRecordsFarmsList: vi.fn(),
  farmRecordsPlotsList: vi.fn(),
  farmRecordsCropCyclesList: vi.fn(),
  farmRecordsHarvestReportsList: vi.fn(),
  farmCropsList: vi.fn(),
}
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsHouseholdsList: (...a: unknown[]) => api.farmRecordsHouseholdsList(...a),
  farmRecordsFarmsList: (...a: unknown[]) => api.farmRecordsFarmsList(...a),
  farmRecordsPlotsList: (...a: unknown[]) => api.farmRecordsPlotsList(...a),
  farmRecordsCropCyclesList: (...a: unknown[]) => api.farmRecordsCropCyclesList(...a),
  farmRecordsHarvestReportsList: (...a: unknown[]) => api.farmRecordsHarvestReportsList(...a),
}))
vi.mock('@genesy/api-client/endpoints/farm-crops', () => ({
  farmCropsList: (...a: unknown[]) => api.farmCropsList(...a),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchMyFarms } = await import('@/features/farmer/useMyFarm')

const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})
const provenance = {
  source: 'farmer_reported',
  verification: 'verified',
  confidence: 'high',
  captured_at: '2026-09-01T08:00:00Z',
  captured_by: null,
  evidence_ref: null,
  verified_at: null,
  verified_by: null,
}

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
  api.farmRecordsHouseholdsList.mockResolvedValue(
    page([
      { id: 'h1', label: 'Mwakalinga household', members: [{ person_id: 'p1', is_head: true }], provenance },
      // A household an officer of another village would also read (mixed roles): not the farmer's.
      { id: 'h2', label: 'Other household', members: [{ person_id: 'p9', is_head: true }], provenance },
    ]),
  )
  api.farmRecordsFarmsList.mockResolvedValue(
    page([
      { id: 'f1', label: 'Riverside farm', household_id: 'h1', managers: [], latitude: null, longitude: null, provenance },
      { id: 'f2', label: 'Hill farm', household_id: null, managers: [{ person_id: 'p1', is_primary: true }], latitude: null, longitude: null, provenance },
      { id: 'f9', label: 'Not mine', household_id: 'h2', managers: [{ person_id: 'p9', is_primary: true }], latitude: null, longitude: null, provenance },
    ]),
  )
  api.farmRecordsPlotsList.mockResolvedValue(
    page([
      { id: 'pl1', farm_id: 'f1', label: 'Riverside', area_ha: '1.8000', provenance },
      { id: 'pl9', farm_id: 'f9', label: 'Not mine', area_ha: null, provenance },
    ]),
  )
  api.farmRecordsCropCyclesList.mockResolvedValue(
    page([
      { id: 'c1', plot_id: 'pl1', crop_id: 'cr-mz', season_label: null, area_ha: '1.6000', tree_count: null, unit_count: null, harvest_start: '2026-09-01', harvest_end: '2026-09-30', provenance },
      { id: 'c9', plot_id: 'pl9', crop_id: 'cr-mz', season_label: null, area_ha: null, tree_count: null, unit_count: null, harvest_start: null, harvest_end: null, provenance },
    ]),
  )
  api.farmRecordsHarvestReportsList.mockResolvedValue(
    page([
      { id: 'hr-a', kind: 'actual', is_current: true, quantity_kg: '3900.00', provenance },
      { id: 'hr-e', kind: 'expected', is_current: true, quantity_kg: '4100.00', provenance },
    ]),
  )
  api.farmCropsList.mockResolvedValue(page([{ id: 'cr-mz', name_en: 'Maize', name_sw: 'Mahindi', is_active: true }]))
})

test('keeps only the farms of this person: managed, or owned by their household', async () => {
  const farms = await fetchMyFarms('p1', 'en')

  expect(farms.map((f) => f.id)).toEqual(['f2', 'f1'])
  expect(farms.find((f) => f.id === 'f1')?.household_label).toBe('Mwakalinga household')
  expect(farms.find((f) => f.id === 'f2')?.household_label).toBeNull()
})

test('nests plots and crop cycles with the crop name and the current expected figure', async () => {
  const farms = await fetchMyFarms('p1', 'sw')
  const riverside = farms.find((f) => f.id === 'f1')!

  expect(riverside).toMatchObject({ label: 'Riverside farm', verification: 'verified', captured_at: '2026-09-01T08:00:00Z' })
  expect(riverside.plots).toHaveLength(1)
  expect(riverside.plots[0]).toMatchObject({ id: 'pl1', label: 'Riverside', area_ha: 1.8 })
  expect(riverside.plots[0].cycles).toHaveLength(1)
  expect(riverside.plots[0].cycles[0]).toMatchObject({ id: 'c1', crop_name: 'Mahindi', area_ha: 1.6 })
  expect(riverside.plots[0].cycles[0].expected).toMatchObject({ id: 'hr-e', quantity_kg: 4100 })
})

test('reads the current figures once per own cycle, never for anyone else’s', async () => {
  await fetchMyFarms('p1', 'en')

  expect(api.farmRecordsHarvestReportsList).toHaveBeenCalledTimes(1)
  expect(api.farmRecordsHarvestReportsList).toHaveBeenCalledWith('w1', 'c1', { current: true, page_size: 100 })
  expect(api.farmRecordsFarmsList).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
})

test('a cycle with no current expected figure has none', async () => {
  api.farmRecordsHarvestReportsList.mockResolvedValue(page([]))
  const farms = await fetchMyFarms('p1', 'en')
  expect(farms.find((f) => f.id === 'f1')?.plots[0].cycles[0].expected).toBeNull()
})
