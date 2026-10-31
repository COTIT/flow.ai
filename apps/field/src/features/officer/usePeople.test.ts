import { describe, expect, test, vi } from 'vitest'

const farmRecordsPersonsList = vi.fn()
vi.mock('@genesy/api-client/endpoints/farm-records', () => ({
  farmRecordsPersonsList: (...args: unknown[]) => farmRecordsPersonsList(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchPeople } = await import('@/features/officer/usePeople')

const provenance = {
  source: 'field_verified', captured_at: '2026-10-01T08:00:00Z', captured_by: { id: 'u1', name: 'Officer' },
  verification: 'unverified', verified_by: null, verified_at: null, confidence: 'high', evidence_ref: null,
}
const person = (id: string, given_name: string, family_name: string) => ({
  id, given_name, family_name, phone: '+255712345678', village_id: 'v1', provenance,
  created_at: '2026-10-01T08:00:00Z', updated_at: '2026-10-01T08:00:00Z',
})
const page = (data: unknown[], current: number, last: number) => ({
  data, meta: { current_page: current, last_page: last, per_page: 100, total: 3, from: null, to: null },
})

describe('fetchPeople', () => {
  test('sends the verification filter, reads every page, flattens provenance and sorts by name', async () => {
    farmRecordsPersonsList
      .mockResolvedValueOnce(page([person('p1', 'Neema', 'Mwakalinga'), person('p2', 'Juma', 'Abdallah')], 1, 2))
      .mockResolvedValueOnce(page([person('p3', 'Asha', 'Mwakalinga')], 2, 2))
    const rows = await fetchPeople({ verification: 'unverified' })
    expect(farmRecordsPersonsList).toHaveBeenNthCalledWith(1, 'w1', { page: 1, page_size: 100, verification: 'unverified' })
    expect(farmRecordsPersonsList).toHaveBeenNthCalledWith(2, 'w1', { page: 2, page_size: 100, verification: 'unverified' })
    expect(rows.map((r) => r.id)).toEqual(['p2', 'p3', 'p1'])
    expect(rows[0]).toEqual({
      id: 'p2', given_name: 'Juma', family_name: 'Abdallah', phone: '+255712345678', village_id: 'v1',
      source: 'field_verified', verification: 'unverified', confidence: 'high', captured_at: '2026-10-01T08:00:00Z',
    })
  })

  test('matches a name search on either name part, any case, and sends no filter when none is set', async () => {
    farmRecordsPersonsList.mockReset()
    farmRecordsPersonsList.mockResolvedValue(page([person('p1', 'Neema', 'Mwakalinga'), person('p2', 'Juma', 'Abdallah')], 1, 1))
    expect((await fetchPeople({ q: 'mwaka' })).map((r) => r.id)).toEqual(['p1'])
    expect((await fetchPeople({ q: 'JUMA' })).map((r) => r.id)).toEqual(['p2'])
    expect(farmRecordsPersonsList).toHaveBeenCalledWith('w1', { page: 1, page_size: 100 })
  })
})
