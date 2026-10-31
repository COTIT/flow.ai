import { describe, expect, test, vi } from 'vitest'

const list = { persons: vi.fn(), households: vi.fn(), farms: vi.fn(), plots: vi.fn(), cycles: vi.fn() }
vi.mock('@genesy/api-client/endpoints/farm-records', async (original) => ({
  ...(await original<object>()),
  farmRecordsPersonsList: (...args: unknown[]) => list.persons(...args),
  farmRecordsHouseholdsList: (...args: unknown[]) => list.households(...args),
  farmRecordsFarmsList: (...args: unknown[]) => list.farms(...args),
  farmRecordsPlotsList: (...args: unknown[]) => list.plots(...args),
  farmRecordsCropCyclesList: (...args: unknown[]) => list.cycles(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchOfficerHome } = await import('@/features/officer/useOfficerHome')

const total = (n: number) => ({ data: [], meta: { current_page: 1, last_page: n, per_page: 1, total: n, from: null, to: null } })

describe('fetchOfficerHome', () => {
  // Counts are the API's totals (one row asked for), never rows counted here; the outstanding
  // figure counts the same kinds and states the verify queue lists.
  test('asks the API for each village\'s totals', async () => {
    list.persons.mockImplementation((_w, p: { verification?: string }) => Promise.resolve(total(p.verification ? 2 : 6)))
    list.farms.mockImplementation((_w, p: { verification?: string }) => Promise.resolve(total(p.verification ? 1 : 4)))
    list.households.mockResolvedValue(total(3))
    list.plots.mockResolvedValue(total(0))
    list.cycles.mockResolvedValue(total(5))

    expect(await fetchOfficerHome(['v1'])).toEqual([{ villageId: 'v1', persons: 6, farms: 4, unverified: 11 }])
    expect(list.persons).toHaveBeenCalledWith('w1', { village_id: 'v1', page_size: 1 })
    expect(list.cycles).toHaveBeenCalledWith('w1', { village_id: 'v1', page_size: 1, verification: 'unverified,pending' })
  })

  test('a failed count is an error, not zero', async () => {
    const failed = { response: { status: 500, data: { message: 'count failed' } } }
    list.plots.mockRejectedValue(failed)
    await expect(fetchOfficerHome(['v1'])).rejects.toBe(failed)
  })
})
