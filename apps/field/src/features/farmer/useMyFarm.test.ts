import { beforeEach, expect, test, vi } from 'vitest'

const holdings = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsPersonsHoldings: (...a: unknown[]) => holdings(...a),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchMyFarms } = await import('@/features/farmer/useMyFarm')

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
const harvest = (id: string, kind: string, is_current: boolean, quantity_kg: string) => ({
  id, kind, is_current, quantity_kg, crop_cycle_id: 'c1', provenance,
})
const cycle = (harvests: unknown[]) => ({
  id: 'c1', plot_id: 'pl1', crop_id: 'cr-mz', season_label: null, area_ha: '1.6000', tree_count: null, unit_count: null,
  harvest_start: '2026-09-01', harvest_end: '2026-09-30', provenance,
  crop: { id: 'cr-mz', code: 'MZ', name_en: 'Maize', name_sw: 'Mahindi' },
  harvests,
})
const reply = (harvests: unknown[]) => ({
  data: {
    households: [{ id: 'h1', label: 'Mwakalinga household', members: [], people: [], provenance }],
    farms: [
      {
        id: 'f1', label: 'Riverside farm', household_id: 'h1', managers: [], provenance,
        plots: [{ id: 'pl1', farm_id: 'f1', label: 'Riverside', area_ha: '1.8000', provenance, cycles: [cycle(harvests)] }],
      },
      { id: 'f2', label: 'Hill farm', household_id: null, managers: [{ person_id: 'p1', is_primary: true }], provenance, plots: [] },
    ],
  },
})

beforeEach(() => {
  holdings.mockReset()
  // Newest first, as the API returns them.
  holdings.mockResolvedValue(
    reply([
      harvest('hr-a', 'actual', true, '3900.00'),
      harvest('hr-e2', 'expected', true, '4100.00'),
      harvest('hr-e1', 'expected', false, '3500.00'),
    ]),
  )
})

test('reads the holdings of this person once', async () => {
  await fetchMyFarms('p1', 'en')
  expect(holdings).toHaveBeenCalledTimes(1)
  expect(holdings).toHaveBeenCalledWith('w1', 'p1')
})

test('lists the farms by label with their household', async () => {
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
  expect(riverside.plots[0].cycles[0].expected).toMatchObject({ id: 'hr-e2', quantity_kg: 4100 })
})

test('a cycle with no current expected figure has none', async () => {
  holdings.mockResolvedValue(reply([harvest('hr-e1', 'expected', false, '3500.00'), harvest('hr-a', 'actual', true, '3900.00')]))
  const farms = await fetchMyFarms('p1', 'en')
  expect(farms.find((f) => f.id === 'f1')?.plots[0].cycles[0].expected).toBeNull()
})
