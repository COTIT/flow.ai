import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmPueRequestsList: vi.fn(),
  farmPueRequestsGet: vi.fn(),
  farmPueRequestsCreate: vi.fn(),
  farmPueRequestsTransition: vi.fn(),
}
vi.mock('@genesy/api-client/endpoints/farm-pue', () => ({
  farmPueRequestsList: (...args: unknown[]) => api.farmPueRequestsList(...args),
  farmPueRequestsGet: (...args: unknown[]) => api.farmPueRequestsGet(...args),
  farmPueRequestsCreate: (...args: unknown[]) => api.farmPueRequestsCreate(...args),
  farmPueRequestsTransition: (...args: unknown[]) => api.farmPueRequestsTransition(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchMyRequests, fetchRequest, submitRequest, transitionRequest, toRequest } = await import(
  '@/features/farmer/useRequests'
)

const ID = '61000000-0000-4000-8000-000000000001'
const pueRequest = (over: Record<string, unknown> = {}) => ({
  id: ID,
  village_id: 'v1',
  person: { id: 'p1', name: 'Neema Mwakalinga' },
  farm_id: null,
  equipment: { id: 'eq1', code: 'MILL', name_en: 'Maize mill', name_sw: 'Kinu' },
  acquisition: 'buy',
  quantity: 1,
  hours_per_day: '8.00',
  days_per_week: '6.0',
  purpose: 'Milling',
  status: 'submitted',
  submitted_at: '2026-10-01T08:00:00Z',
  decided_at: null,
  decided_by: null,
  decision_note: null,
  estimate: {
    rated_power_kw: '15.000', quantity: 1, hours_per_day: '8.00', days_per_week: '6.0',
    est_power_kw: '15.000', est_kwh_per_day: '120.000', est_kwh_per_week: '720.000',
    method: 'level1_rated_x_hours', source: 'model_estimated', confidence: null, computed_at: '2026-10-01T08:00:00Z',
  },
  provenance: {
    source: 'farmer_reported', verification: 'unverified', confidence: null, evidence_ref: null,
    captured_at: '2026-10-01T08:00:00Z', captured_by: { id: 'u1', name: 'Neema Mwakalinga' },
    verified_at: null, verified_by: null,
  },
  created_at: '2026-10-01T08:00:00Z',
  updated_at: '2026-10-01T08:00:00Z',
  ...over,
})
const page = (data: unknown[], current = 1, last = 1) => ({
  data,
  meta: { current_page: current, last_page: last, per_page: 100, from: 1, to: data.length, total: data.length },
})

beforeEach(() => {
  for (const fn of Object.values(api)) fn.mockReset()
})

describe('the farmer’s requests', () => {
  test('reads every page; the API scopes them to the caller', async () => {
    api.farmPueRequestsList
      .mockResolvedValueOnce(page([pueRequest()], 1, 2))
      .mockResolvedValueOnce(page([pueRequest({ id: 'r2' })], 2, 2))

    const rows = await fetchMyRequests()

    expect(rows.map((r) => r.id)).toEqual([ID, 'r2'])
    expect(api.farmPueRequestsList).toHaveBeenLastCalledWith('w1', { page: 2, page_size: 100 })
  })

  test('one is read by id; a 404 or a malformed id is "not found"', async () => {
    api.farmPueRequestsGet.mockResolvedValueOnce({ data: pueRequest() })
    expect((await fetchRequest(ID))?.id).toBe(ID)
    expect(api.farmPueRequestsGet).toHaveBeenCalledWith('w1', ID)

    api.farmPueRequestsGet.mockRejectedValueOnce({ response: { status: 404, data: { message: 'Not found.' } } })
    expect(await fetchRequest(ID)).toBeNull()

    expect(await fetchRequest('not-a-uuid')).toBeNull()
    expect(api.farmPueRequestsGet).toHaveBeenCalledTimes(2)
  })
})

describe('the shape the screens read', () => {
  test('names, provenance and the API’s stored estimate, as numbers for display', () => {
    const r = toRequest(pueRequest() as never, 'sw')

    expect(r.equipment_name).toBe('Kinu')
    expect(r.applicant).toBe('Neema Mwakalinga')
    expect(r.captured_by_name).toBe('Neema Mwakalinga')
    expect(r.source).toBe('farmer_reported')
    expect(r.hours_per_day).toBe(8)
    expect(r.estimate).toMatchObject({ est_power_kw: 15, est_kwh_per_day: 120, est_kwh_per_week: 720 })
  })

  test('a machine with no rated power has no estimate', () => {
    expect(toRequest(pueRequest({ estimate: null, hours_per_day: null }) as never, 'en').estimate).toBeNull()
  })
})

describe('filing a request', () => {
  test('sends the client id and what was asked for, never a person', async () => {
    api.farmPueRequestsCreate.mockResolvedValue({ data: pueRequest() })

    await submitRequest({
      id: ID,
      equipmentId: 'eq1',
      acquisition: 'rent',
      quantity: '2',
      hoursPerDay: '6',
      daysPerWeek: '5.5',
      purpose: '',
    })

    expect(api.farmPueRequestsCreate).toHaveBeenCalledWith('w1', {
      id: ID,
      equipment_id: 'eq1',
      acquisition: 'rent',
      quantity: 2,
      hours_per_day: '6',
      days_per_week: '5.5',
      purpose: null,
      status: 'submitted',
    })
    expect(api.farmPueRequestsCreate.mock.calls[0][1]).not.toHaveProperty('person_id')
  })
})

describe('moving a request', () => {
  test('posts the action; the API decides whether it is allowed', async () => {
    api.farmPueRequestsTransition.mockResolvedValue({ data: pueRequest({ status: 'withdrawn' }) })

    await transitionRequest(ID, 'withdraw')
    await transitionRequest(ID, 'approve', 'Fits the headroom')

    expect(api.farmPueRequestsTransition).toHaveBeenNthCalledWith(1, 'w1', ID, { action: 'withdraw' })
    expect(api.farmPueRequestsTransition).toHaveBeenNthCalledWith(2, 'w1', ID, { action: 'approve', note: 'Fits the headroom' })
  })
})
