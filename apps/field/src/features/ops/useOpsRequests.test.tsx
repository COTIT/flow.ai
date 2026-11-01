import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = { farmPueRequestsList: vi.fn(), farmPueVillageEnergy: vi.fn() }
vi.mock('@genesy/api-client/endpoints/farm-pue', () => ({
  farmPueRequestsList: (...args: unknown[]) => api.farmPueRequestsList(...args),
  farmPueVillageEnergy: (...args: unknown[]) => api.farmPueVillageEnergy(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchOpsRequests, fetchVillageEnergy } = await import('@/features/ops/useOpsRequests')

const VILLAGE = '30000000-0000-4000-8000-000000000001'
const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})

beforeEach(() => {
  api.farmPueRequestsList.mockReset()
  api.farmPueVillageEnergy.mockReset()
})

describe('the pipeline', () => {
  test('filters on the API, not on fetched rows', async () => {
    api.farmPueRequestsList.mockResolvedValue(page([]))

    await fetchOpsRequests({ status: 'submitted', village: VILLAGE })
    await fetchOpsRequests({})

    expect(api.farmPueRequestsList).toHaveBeenNthCalledWith(1, 'w1', {
      status: 'submitted',
      village_id: VILLAGE,
      page: 1,
      page_size: 100,
    })
    expect(api.farmPueRequestsList).toHaveBeenNthCalledWith(2, 'w1', { page: 1, page_size: 100 })
  })
})

describe('village energy', () => {
  test('is the API’s figures, as numbers for display', async () => {
    api.farmPueVillageEnergy.mockResolvedValue({
      data: {
        village_id: VILLAGE, capacity_kw: '500.00', capacity_basis: 'planned', simultaneity_factor: '0.600',
        prospective_kw_raw: '15.000', approved_kw_raw: '18.000', prospective_peak_kw: '9.000',
        approved_peak_kw: '10.800', headroom_kw: '489.200', approved_kwh_per_week: '828.000', approved_assets: 2,
      },
    })

    const energy = await fetchVillageEnergy(VILLAGE)

    expect(api.farmPueVillageEnergy).toHaveBeenCalledWith('w1', VILLAGE)
    expect(energy).toMatchObject({ capacity_kw: 500, headroom_kw: 489.2, approved_peak_kw: 10.8, simultaneity_factor: 0.6, capacity_basis: 'planned' })
  })
})
