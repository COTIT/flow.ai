import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

type Result = { data: unknown; error: { message: string } | null }

const responses = new Map<string, Result>()
const calls: Array<{ table: string; method: string; args: unknown[] }> = []
const rpc = vi.fn()
const from = vi.fn((table: string) => {
  const chain: Record<string, unknown> = {}
  for (const method of ['select', 'eq', 'is', 'order', 'in']) {
    chain[method] = vi.fn((...args: unknown[]) => {
      calls.push({ table, method, args })
      return chain
    })
  }
  chain.then = (resolve: (value: unknown) => unknown, reject?: (reason: unknown) => unknown) =>
    Promise.resolve(responses.get(table) ?? { data: [], error: null }).then(resolve, reject)
  return chain
})

vi.mock('@/lib/supabase', () => ({ supabase: { from, rpc } }))

const {
  fetchFarmerVouchers,
  fetchVoucherCode,
  fetchVoucherTimeline,
  redeemedByName,
  useVoucher,
  voucherDisplayStatus,
} = await import('@/features/farmer/useVouchers')

const VOUCHER = '51000000-0000-4000-8000-000000000001'
const OTHER = '51000000-0000-4000-8000-000000000002'

const voucher = (over: Record<string, unknown> = {}) => ({
  id: VOUCHER,
  response_id: 'r1',
  survey_id: 's1',
  household_id: 'h1',
  village_id: 'v1',
  amount: 5000,
  currency: 'TZS',
  status: 'issued',
  issued_at: '2026-09-20T08:00:00Z',
  expires_at: '2026-10-20T08:00:00Z',
  redeemed_by: null,
  redeemed_at: null,
  id_type_seen: null,
  voided_by: null,
  voided_at: null,
  void_reason: null,
  ...over,
})

beforeEach(() => {
  responses.clear()
  calls.length = 0
  rpc.mockReset()
  from.mockClear()
})

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  )
}

/**
 * The household's own vouchers. `survey_voucher` grants a farmer a column
 * list that leaves out `code` and `audit_required`: asking for either — or
 * for `*` — is refused with "permission denied", so the select string is the
 * thing under test here.
 */
describe('the household vouchers', () => {
  test('selects only the columns a farmer is granted', async () => {
    responses.set('survey_voucher', { data: [voucher()], error: null })
    await fetchFarmerVouchers()

    const select = calls.find((c) => c.table === 'survey_voucher' && c.method === 'select')
    const columns = String(select?.args[0])
    expect(columns).not.toMatch(/\*/)
    expect(columns).not.toMatch(/\bcode\b/)
    expect(columns).not.toMatch(/audit_required/)
    for (const column of ['id', 'status', 'amount', 'currency', 'expires_at', 'redeemed_at', 'void_reason']) {
      expect(columns).toMatch(new RegExp(`\\b${column}\\b`))
    }
  })

  // RLS scopes the rows to the caller's household. A client-side filter would
  // imply the client is what keeps other households' vouchers out.
  test('never filters by household in the client', async () => {
    await fetchFarmerVouchers()
    expect(calls.filter((c) => c.method === 'eq')).toEqual([])
  })

  test('amounts arrive as numbers even when PostgREST sends numeric as text', async () => {
    responses.set('survey_voucher', { data: [voucher({ amount: '5000.00' })], error: null })
    const [row] = await fetchFarmerVouchers()
    expect(row.amount).toBe(5000)
  })

  test('a failed read rejects rather than reading as no vouchers', async () => {
    responses.set('survey_voucher', { data: null, error: { message: 'permission denied for table survey_voucher' } })
    await expect(fetchFarmerVouchers()).rejects.toThrow('permission denied for table survey_voucher')
  })

  test('no rows is an empty list', async () => {
    responses.set('survey_voucher', { data: null, error: null })
    await expect(fetchFarmerVouchers()).resolves.toEqual([])
  })

  test('one voucher is picked out of the household list by id', async () => {
    responses.set('survey_voucher', { data: [voucher({ id: OTHER }), voucher()], error: null })
    const { result } = renderHook(() => useVoucher(VOUCHER), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data?.id).toBe(VOUCHER)
  })

  // Zero rows is an answer: a voucher RLS does not show is simply absent.
  test('a voucher that is not visible is null, not an error', async () => {
    responses.set('survey_voucher', { data: [voucher({ id: OTHER })], error: null })
    const { result } = renderHook(() => useVoucher(VOUCHER), { wrapper: wrapper() })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(result.current.data).toBeNull()
  })
})

describe('the voucher code', () => {
  test('is read through app_voucher_code, never from the table', async () => {
    rpc.mockResolvedValue({ data: 'K7QXM2PA9D', error: null })
    await expect(fetchVoucherCode(VOUCHER)).resolves.toBe('K7QXM2PA9D')
    expect(rpc).toHaveBeenCalledWith('app_voucher_code', { p_voucher_id: VOUCHER })
    expect(from).not.toHaveBeenCalled()
  })

  // NULL for anyone outside the household. Not an error: nothing to show.
  test('null is an answer', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    await expect(fetchVoucherCode(VOUCHER)).resolves.toBeNull()
  })

  test('a malformed id reaches the same answer without a round trip', async () => {
    await expect(fetchVoucherCode('not-a-uuid')).resolves.toBeNull()
    expect(rpc).not.toHaveBeenCalled()
  })

  test('a failure rejects with the database message', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'sign in first' } })
    await expect(fetchVoucherCode(VOUCHER)).rejects.toThrow('sign in first')
  })
})

describe('the voucher timeline', () => {
  test('is read through app_voucher_timeline and keeps the database order', async () => {
    rpc.mockResolvedValue({
      error: null,
      data: [
        { occurred_at: '2026-09-20T08:00:00Z', kind: 'answered', actor_name: 'Amina', actor_role: 'farmer', detail: {} },
        { occurred_at: '2026-09-20T08:00:01Z', kind: 'issued', actor_name: 'Amina', actor_role: 'farmer', detail: {} },
      ],
    })
    const events = await fetchVoucherTimeline(VOUCHER)

    expect(rpc).toHaveBeenCalledWith('app_voucher_timeline', { p_voucher_id: VOUCHER })
    expect(events.map((e) => e.kind)).toEqual(['answered', 'issued'])
  })

  test('a detail that is not an object is dropped rather than rendered', async () => {
    rpc.mockResolvedValue({
      error: null,
      data: [
        { occurred_at: '2026-09-20T08:00:00Z', kind: 'voided', actor_name: 'Asha', actor_role: 'ops', detail: { reason: 'duplicate household' } },
        { occurred_at: '2026-09-20T08:00:01Z', kind: 'issued', actor_name: null, actor_role: null, detail: 'x' },
      ],
    })
    const [voided, issued] = await fetchVoucherTimeline(VOUCHER)
    expect(voided.detail).toEqual({ reason: 'duplicate household' })
    expect(issued.detail).toBeNull()
    expect(issued.actor_name).toBeNull()
  })

  test('nothing visible is an empty trail', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    await expect(fetchVoucherTimeline(VOUCHER)).resolves.toEqual([])
  })

  test('a malformed id is an empty trail without a round trip', async () => {
    await expect(fetchVoucherTimeline('nope')).resolves.toEqual([])
    expect(rpc).not.toHaveBeenCalled()
  })

  test('a failure rejects with the database message', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'timeline failed' } })
    await expect(fetchVoucherTimeline(VOUCHER)).rejects.toThrow('timeline failed')
  })

  test('the name on the hand-over is the latest redeemed event', () => {
    expect(
      redeemedByName([
        { occurred_at: '1', kind: 'issued', actor_name: 'Amina', actor_role: 'farmer', detail: null },
        { occurred_at: '2', kind: 'redeemed', actor_name: 'Juma Officer', actor_role: 'field_officer', detail: null },
      ]),
    ).toBe('Juma Officer')
    expect(redeemedByName([])).toBeNull()
  })
})

/**
 * `expired` is not stored: the database keeps `issued` and an `expires_at`.
 * Only an uncollected voucher can expire — a collected or cancelled one keeps
 * the status that says what happened to it.
 */
describe('the status a voucher is shown with', () => {
  const now = Date.parse('2026-10-01T00:00:00Z')

  test('issued and in date', () => {
    expect(voucherDisplayStatus({ status: 'issued', expires_at: '2026-10-20T00:00:00Z' }, now)).toBe('issued')
  })

  test('issued and past its date is expired', () => {
    expect(voucherDisplayStatus({ status: 'issued', expires_at: '2026-09-30T00:00:00Z' }, now)).toBe('expired')
  })

  test('expiring at this very moment is expired', () => {
    expect(voucherDisplayStatus({ status: 'issued', expires_at: '2026-10-01T00:00:00Z' }, now)).toBe('expired')
  })

  test('collected and cancelled never read as expired', () => {
    expect(voucherDisplayStatus({ status: 'redeemed', expires_at: '2026-09-01T00:00:00Z' }, now)).toBe('redeemed')
    expect(voucherDisplayStatus({ status: 'void', expires_at: '2026-09-01T00:00:00Z' }, now)).toBe('void')
  })
})
