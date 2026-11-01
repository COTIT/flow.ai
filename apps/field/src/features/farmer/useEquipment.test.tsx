import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = { farmEquipmentList: vi.fn(), farmEquipmentGet: vi.fn() }
vi.mock('@genesy/api-client/endpoints/farm-equipment', () => ({
  farmEquipmentList: (...args: unknown[]) => api.farmEquipmentList(...args),
  farmEquipmentGet: (...args: unknown[]) => api.farmEquipmentGet(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchEquipment, fetchEquipmentItem, localise } = await import('@/features/farmer/useEquipment')

const ID = '51000000-0000-4000-8000-000000000001'
const tzs = (minor: number) => ({ minor, currency: 'TZS', decimals: 2, currency_id: 'c1' })
const machine = (over: Record<string, unknown> = {}) => ({
  id: ID,
  category: { id: 'cat1', code: 'MILLING', name_en: 'Milling', name_sw: 'Kusaga' },
  code: 'MILL-500',
  name_en: 'Maize mill',
  name_sw: 'Mashine ya kusaga',
  rated_power_kw: '15.000',
  typical_hours_per_day: '6.00',
  typical_days_per_week: '5.0',
  can_rent: true,
  can_buy: true,
  indicative_price: tzs(2200000000),
  indicative_rent_per_day: null,
  is_active: true,
  created_at: '2026-10-01T00:00:00Z',
  updated_at: '2026-10-01T00:00:00Z',
  ...over,
})
const page = (data: unknown[], current = 1, last = 1) => ({
  data,
  meta: { current_page: current, last_page: last, per_page: 100, from: 1, to: data.length, total: data.length },
})

beforeEach(() => {
  api.farmEquipmentList.mockReset()
  api.farmEquipmentGet.mockReset()
})

describe('the equipment list', () => {
  test('reads every page of active machines', async () => {
    api.farmEquipmentList
      .mockResolvedValueOnce(page([machine()], 1, 2))
      .mockResolvedValueOnce(page([machine({ id: 'x2' })], 2, 2))

    const rows = await fetchEquipment()

    expect(rows.map((r) => r.id)).toEqual([ID, 'x2'])
    expect(api.farmEquipmentList).toHaveBeenLastCalledWith('w1', { active: true, page: 2, page_size: 100 })
  })
})

describe('one machine', () => {
  test('is read by id', async () => {
    api.farmEquipmentGet.mockResolvedValue({ data: machine() })

    expect((await fetchEquipmentItem(ID))?.code).toBe('MILL-500')
    expect(api.farmEquipmentGet).toHaveBeenCalledWith('w1', ID)
  })

  test('a malformed id is "not found" without a call', async () => {
    expect(await fetchEquipmentItem('not-a-uuid')).toBeNull()
    expect(api.farmEquipmentGet).not.toHaveBeenCalled()
  })

  test('a 404 (unknown, or another account) is "not found"', async () => {
    api.farmEquipmentGet.mockRejectedValue({ isAxiosError: true, response: { status: 404, data: { message: 'Not found.' } } })

    expect(await fetchEquipmentItem(ID)).toBeNull()
  })

  test('any other failure is an error', async () => {
    api.farmEquipmentGet.mockRejectedValue({ isAxiosError: true, response: { status: 500, data: {} } })

    await expect(fetchEquipmentItem(ID)).rejects.toBeTruthy()
  })
})

describe('localise', () => {
  test('names the machine and its category in the reader’s language', () => {
    expect(localise(machine(), 'sw')).toMatchObject({ name: 'Mashine ya kusaga', category_name: 'Kusaga' })
    expect(localise(machine(), 'en')).toMatchObject({ name: 'Maize mill', category_name: 'Milling' })
  })

  test('keeps prices as the API sent them, and absent figures absent', () => {
    const item = localise(machine({ rated_power_kw: null, typical_hours_per_day: null }), 'en')

    expect(item.indicative_price).toEqual(tzs(2200000000))
    expect(item.indicative_rent_per_day).toBeNull()
    expect(item.rated_power_kw).toBeNull()
    expect(item.typical_hours_per_day).toBeNull()
    expect(item.typical_days_per_week).toBe(5)
  })
})
