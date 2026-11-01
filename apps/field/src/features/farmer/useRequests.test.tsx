import { createElement, type ReactNode } from 'react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmPueRequestsList: vi.fn(),
  farmPueRequestsGet: vi.fn(),
  farmPueRequestsCreate: vi.fn(),
  farmPueRequestsTransition: vi.fn(),
  farmPueRequestsEstimate: vi.fn(),
}
vi.mock('@genesy/api-client/endpoints/farm-pue', () => ({
  farmPueRequestsList: (...args: unknown[]) => api.farmPueRequestsList(...args),
  farmPueRequestsGet: (...args: unknown[]) => api.farmPueRequestsGet(...args),
  farmPueRequestsCreate: (...args: unknown[]) => api.farmPueRequestsCreate(...args),
  farmPueRequestsTransition: (...args: unknown[]) => api.farmPueRequestsTransition(...args),
  farmPueRequestsEstimate: (...args: unknown[]) => api.farmPueRequestsEstimate(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const { fetchEstimatePreview, fetchMyRequests, fetchRequest, submitRequest, transitionRequest, toRequest, useEstimatePreview } = await import(
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

  test('the farm is named by the API', () => {
    expect(toRequest(pueRequest({ farm_id: 'f1', farm: { id: 'f1', label: 'Kipande' } }) as never, 'en').farm_label).toBe('Kipande')
    expect(toRequest(pueRequest({ farm: null }) as never, 'en').farm_label).toBeNull()
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

const preview = {
  rated_power_kw: '15.000', quantity: 2, hours_per_day: '8.00', days_per_week: '5.0',
  est_power_kw: '30.000', est_kwh_per_day: '240.000', est_kwh_per_week: '1200.000', method: 'level1_rated_x_hours',
}

describe('the estimate preview', () => {
  test('asks the API with the figures as typed (hours and days as decimal text) and shows its answer', async () => {
    api.farmPueRequestsEstimate.mockResolvedValue({ data: preview })

    const estimate = await fetchEstimatePreview({ equipmentId: 'eq1', quantity: '2', hoursPerDay: '8', daysPerWeek: '5' })

    expect(api.farmPueRequestsEstimate).toHaveBeenCalledWith('w1', {
      equipment_id: 'eq1', quantity: 2, hours_per_day: '8', days_per_week: '5',
    })
    expect(estimate).toEqual({
      rated_power_kw: 15, quantity: 2, hours_per_day: 8, days_per_week: 5,
      est_power_kw: 30, est_kwh_per_day: 240, est_kwh_per_week: 1200,
    })
  })

  test('a machine with no rated power has none', async () => {
    api.farmPueRequestsEstimate.mockResolvedValue({ data: null })
    expect(await fetchEstimatePreview({ equipmentId: 'eq1', quantity: '1', hoursPerDay: '6', daysPerWeek: '5' })).toBeNull()
  })

  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: new QueryClient({ defaultOptions: { queries: { retry: false } } }) }, children)
  const input = (hoursPerDay: string) => ({ equipmentId: 'eq1', quantity: '1', hoursPerDay, daysPerWeek: '5' })

  test('waits for typing to settle: only the figures the farmer stopped on are asked about', async () => {
    api.farmPueRequestsEstimate.mockResolvedValue({ data: preview })
    const { result, rerender } = renderHook(({ value }) => useEstimatePreview(value), {
      wrapper,
      initialProps: { value: input('6') as ReturnType<typeof input> | null },
    })
    await waitFor(() => expect(result.current.data).toBeTruthy())

    rerender({ value: input('7') })
    rerender({ value: input('8') })
    await waitFor(() => expect(api.farmPueRequestsEstimate).toHaveBeenCalledTimes(2))
    await new Promise((resolve) => setTimeout(resolve, 400))

    expect(api.farmPueRequestsEstimate.mock.calls.map((c) => c[1].hours_per_day)).toEqual(['6', '8'])
  })

  test('asks nothing while the figures are missing or impossible', async () => {
    renderHook(() => useEstimatePreview(null), { wrapper })
    await new Promise((resolve) => setTimeout(resolve, 400))
    expect(api.farmPueRequestsEstimate).not.toHaveBeenCalled()
  })
})
