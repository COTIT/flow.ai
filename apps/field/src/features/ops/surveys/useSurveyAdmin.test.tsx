import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

type Response = { data: unknown; error: { message: string } | null }

/** Responses per table; `single` answers maybeSingle()/single() chains. */
const responses = new Map<string, Response>()
const singles = new Map<string, Response>()
const calls: Array<{ table: string; method: string; args: unknown[] }> = []
const rpc = vi.fn()

const from = vi.fn((table: string) => {
  let single = false
  const chain: Record<string, unknown> = {}
  for (const method of ['select', 'eq', 'is', 'in', 'order', 'insert', 'update']) {
    chain[method] = vi.fn((...args: unknown[]) => {
      calls.push({ table, method, args })
      return chain
    })
  }
  for (const method of ['maybeSingle', 'single']) {
    chain[method] = vi.fn(() => {
      single = true
      return chain
    })
  }
  chain.then = (resolve: (value: unknown) => unknown, reject?: (reason: unknown) => unknown) =>
    Promise.resolve(
      (single ? singles.get(table) : responses.get(table)) ?? { data: single ? null : [], error: null },
    ).then(resolve, reject)
  return chain
})

vi.mock('@/lib/supabase', () => ({ supabase: { from, rpc } }))

const hooks = await import('@/features/ops/surveys/useSurveyAdmin')
const { queryKeys } = await import('@/lib/queryKeys')
await import('@/i18n')

const surveyId = '30000000-0000-4000-8000-000000000001'
const voucherId = '40000000-0000-4000-8000-000000000001'

function client() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const invalidate = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue()
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
  return { queryClient, invalidate, wrapper }
}

const invalidatedKeys = (invalidate: { mock: { calls: unknown[][] } }) =>
  invalidate.mock.calls.map((call) => (call[0] as { queryKey: unknown }).queryKey)

beforeEach(() => {
  responses.clear()
  singles.clear()
  calls.length = 0
  rpc.mockReset()
  from.mockClear()
})

describe('the survey list', () => {
  test('pairs each survey with its database summary, without counting anything', async () => {
    responses.set('survey', {
      error: null,
      data: [
        { id: 's1', title_en: 'Harvest', title_sw: null, status: 'live', reward_amount: 5000, currency: 'TZS' },
        { id: 's2', title_en: 'Draft', title_sw: null, status: 'draft', reward_amount: 2000, currency: 'TZS' },
      ],
    })
    responses.set('v_survey_summary', {
      error: null,
      data: [{ survey_id: 's1', responses: 3, redeemed_count: 1, redeemed_amount: 5000 }],
    })

    const rows = await hooks.fetchSurveyAdminList()

    expect(rows.map((r) => r.id)).toEqual(['s1', 's2'])
    expect(rows[0].summary).toMatchObject({ responses: 3, redeemed_amount: 5000 })
    // No summary row is an answer, not a zero the client invented.
    expect(rows[1].summary).toBeNull()
  })

  test('asks for both title columns and lets the screen choose', async () => {
    await hooks.fetchSurveyAdminList()

    const select = calls.find((c) => c.table === 'survey' && c.method === 'select')
    expect(String(select?.args[0])).toMatch(/title_en/)
    expect(String(select?.args[0])).toMatch(/title_sw/)
  })

  test('a failed read rejects rather than showing an empty list', async () => {
    responses.set('v_survey_summary', { data: null, error: { message: 'summary read failed' } })
    await expect(hooks.fetchSurveyAdminList()).rejects.toThrow('summary read failed')
  })

  test('is cached under the surveyAdmin key', async () => {
    responses.set('survey', { error: null, data: [{ id: 's1' }] })
    const { queryClient, wrapper } = client()
    const { result } = renderHook(() => hooks.useSurveyAdminList(), { wrapper })

    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(queryClient.getQueryData(queryKeys.surveyAdmin())).toHaveLength(1)
  })
})

describe('one survey', () => {
  test('reads the survey, its live questions in order, and its summary', async () => {
    singles.set('survey', { error: null, data: { id: surveyId, status: 'draft', title_en: 'Harvest' } })
    responses.set('survey_question', { error: null, data: [{ id: 'q1', position: 1 }] })
    singles.set('v_survey_summary', { error: null, data: { survey_id: surveyId, responses: 0 } })

    const detail = await hooks.fetchSurveyAdminDetail(surveyId)

    expect(detail?.survey.title_en).toBe('Harvest')
    expect(detail?.questions).toHaveLength(1)
    expect(detail?.summary).toMatchObject({ responses: 0 })
    // Removed questions stay in the table with deleted_at set.
    expect(calls).toContainEqual({ table: 'survey_question', method: 'is', args: ['deleted_at', null] })
    expect(calls).toContainEqual({ table: 'survey_question', method: 'order', args: ['position'] })
  })

  test('zero rows is not found, not an error', async () => {
    await expect(hooks.fetchSurveyAdminDetail(surveyId)).resolves.toBeNull()
  })

  test('a malformed id reaches not found without a query', async () => {
    await expect(hooks.fetchSurveyAdminDetail('not-a-uuid')).resolves.toBeNull()
    expect(from).not.toHaveBeenCalled()
  })

  test('a failed question read rejects', async () => {
    singles.set('survey', { error: null, data: { id: surveyId } })
    responses.set('survey_question', { data: null, error: { message: 'questions failed' } })
    await expect(hooks.fetchSurveyAdminDetail(surveyId)).rejects.toThrow('questions failed')
  })
})

describe('results', () => {
  test('the tally and number summary come from their views, filtered to the survey', async () => {
    responses.set('v_survey_answer_tally', {
      error: null,
      data: [{ question_id: 'q1', option_value: 'maize', answer_count: 4 }],
    })
    responses.set('v_survey_number_summary', {
      error: null,
      data: [{ question_id: 'q2', answer_count: 3, average: 2.5, minimum: 1, maximum: 4 }],
    })

    const tally = await hooks.fetchSurveyTally(surveyId)

    expect(tally.choices).toHaveLength(1)
    expect(tally.numbers[0]).toMatchObject({ average: 2.5 })
    expect(calls).toContainEqual({ table: 'v_survey_answer_tally', method: 'eq', args: ['survey_id', surveyId] })
    expect(calls).toContainEqual({ table: 'v_survey_number_summary', method: 'eq', args: ['survey_id', surveyId] })
  })

  test('vouchers come from app_survey_vouchers', async () => {
    rpc.mockResolvedValue({ data: [{ voucher_id: voucherId }], error: null })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSurveyVouchers(surveyId), { wrapper })

    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(rpc).toHaveBeenCalledWith('app_survey_vouchers', { p_survey_id: surveyId })
  })

  test('a voucher timeline is read only once a voucher is chosen', async () => {
    rpc.mockResolvedValue({ data: [{ kind: 'issued', occurred_at: '2026-09-20T00:00:00Z' }], error: null })
    const { wrapper } = client()
    const { result, rerender } = renderHook(({ id }) => hooks.useVoucherTimeline(id), {
      wrapper,
      initialProps: { id: null as string | null },
    })
    expect(rpc).not.toHaveBeenCalled()

    rerender({ id: voucherId })
    await waitFor(() => expect(result.current.data).toHaveLength(1))
    expect(rpc).toHaveBeenCalledWith('app_voucher_timeline', { p_voucher_id: voucherId })
  })
})

describe('authoring', () => {
  test('a new survey is a draft insert of only the client columns', async () => {
    singles.set('survey', { error: null, data: { id: surveyId } })
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useCreateSurvey(), { wrapper })

    await result.current.mutateAsync({ id: surveyId, project_id: 'p1', title_en: 'Harvest', reward_amount: 5000 })

    expect(calls).toContainEqual({
      table: 'survey',
      method: 'insert',
      args: [{ id: surveyId, project_id: 'p1', title_en: 'Harvest', reward_amount: 5000 }],
    })
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([queryKeys.surveyAdmin(), queryKeys.surveyEligibility()]),
    )
  })

  test('the database refusal reaches the caller as written', async () => {
    singles.set('survey', { data: null, error: { message: 'only an admin may author surveys' } })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useCreateSurvey(), { wrapper })

    await expect(
      result.current.mutateAsync({ id: surveyId, project_id: 'p1', title_en: 'Harvest', reward_amount: 5000 }),
    ).rejects.toThrow('only an admin may author surveys')
  })

  test('saving a draft updates the survey and refreshes every survey view', async () => {
    responses.set('survey', { error: null, data: [{ id: surveyId }] })
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useUpdateSurvey(surveyId), { wrapper })

    await result.current.mutateAsync({ title_en: 'Harvest 2026' })

    expect(calls).toContainEqual({ table: 'survey', method: 'update', args: [{ title_en: 'Harvest 2026' }] })
    expect(calls).toContainEqual({ table: 'survey', method: 'eq', args: ['id', surveyId] })
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        queryKeys.surveyAdmin(),
        queryKeys.surveyAdminDetail(surveyId),
        queryKeys.surveyEligibility(),
      ]),
    )
  })

  // RLS filters an update it refuses to zero rows, with no error. That is
  // not a save.
  test('an update that touched no row is refused, not reported as saved', async () => {
    responses.set('survey', { error: null, data: [] })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useUpdateSurvey(surveyId), { wrapper })

    await expect(result.current.mutateAsync({ title_en: 'x' })).rejects.toThrow(/permission/i)
  })

  test('publishing is a status update to live, and the guard message comes through', async () => {
    responses.set('survey', {
      data: null,
      error: { message: 'a survey needs at least one question before it goes live' },
    })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSetSurveyStatus(surveyId), { wrapper })

    await expect(result.current.mutateAsync('live')).rejects.toThrow(
      'a survey needs at least one question before it goes live',
    )
    expect(calls).toContainEqual({ table: 'survey', method: 'update', args: [{ status: 'live' }] })
  })
})

describe('questions', () => {
  const question = {
    kind: 'single_choice' as const,
    prompt_en: 'Main crop?',
    prompt_sw: '',
    required: true,
    options: [{ value: 'maize', label_en: 'Maize' }],
  }

  test('a new question is inserted at the position given', async () => {
    singles.set('survey_question', { error: null, data: { id: 'q9' } })
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useSaveQuestion(surveyId), { wrapper })

    await result.current.mutateAsync({ ...question, position: 3 })

    const insert = calls.find((c) => c.table === 'survey_question' && c.method === 'insert')
    expect(insert?.args[0]).toMatchObject({
      survey_id: surveyId,
      position: 3,
      kind: 'single_choice',
      prompt_en: 'Main crop?',
      // A blank Swahili prompt is absent, not an empty string.
      prompt_sw: null,
      required: true,
      options: [{ value: 'maize', label_en: 'Maize' }],
    })
    expect((insert?.args[0] as { id: string }).id).toMatch(/^[0-9a-f-]{36}$/)
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([queryKeys.surveyAdminDetail(surveyId)]),
    )
  })

  test('an existing question is updated in place, never moved to another survey', async () => {
    responses.set('survey_question', { error: null, data: [{ id: 'q1' }] })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSaveQuestion(surveyId), { wrapper })

    await result.current.mutateAsync({ ...question, id: 'q1', position: 1 })

    const update = calls.find((c) => c.table === 'survey_question' && c.method === 'update')
    expect(update?.args[0]).not.toHaveProperty('survey_id')
    expect(update?.args[0]).not.toHaveProperty('position')
    expect(calls).toContainEqual({ table: 'survey_question', method: 'eq', args: ['id', 'q1'] })
  })

  test('removing a question sets deleted_at: there is no delete', async () => {
    responses.set('survey_question', { error: null, data: [{ id: 'q1' }] })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useRemoveQuestion(surveyId), { wrapper })

    await result.current.mutateAsync('q1')

    const update = calls.find((c) => c.table === 'survey_question' && c.method === 'update')
    expect(update?.args[0]).toEqual({ deleted_at: expect.any(String) })
  })

  test('moving swaps two positions', async () => {
    responses.set('survey_question', { error: null, data: [{ id: 'x' }] })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSwapQuestions(surveyId), { wrapper })

    await result.current.mutateAsync({ first: { id: 'q1', position: 1 }, second: { id: 'q2', position: 2 } })

    const updates = calls.filter((c) => c.table === 'survey_question' && c.method === 'update')
    expect(updates.map((c) => c.args[0])).toEqual([{ position: 2 }, { position: 1 }])
  })
})

describe('voiding a voucher', () => {
  test('calls app_voucher_void and refreshes the voucher, its trail, the summary and the log', async () => {
    rpc.mockResolvedValue({ data: null, error: null })
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useVoidVoucher(surveyId), { wrapper })

    await result.current.mutateAsync({ voucherId, reason: 'Duplicate household' })

    expect(rpc).toHaveBeenCalledWith('app_voucher_void', {
      p_voucher_id: voucherId,
      p_reason: 'Duplicate household',
    })
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        queryKeys.surveyVouchers(surveyId),
        queryKeys.surveyAdmin(),
        queryKeys.voucherTimeline(voucherId),
        ['redemptionLog'],
      ]),
    )
  })

  // The reason is required by the database, and its sentence says so.
  test('a blank reason is sent, and refused by the database in its words', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'give a reason for voiding this voucher' } })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useVoidVoucher(surveyId), { wrapper })

    await expect(result.current.mutateAsync({ voucherId, reason: '' })).rejects.toThrow(
      'give a reason for voiding this voucher',
    )
    expect(rpc).toHaveBeenCalledWith('app_voucher_void', { p_voucher_id: voucherId, p_reason: '' })
  })
})

describe('redemptions', () => {
  test('reads totals and the log for the same Tanzanian date range', async () => {
    rpc.mockImplementation(async (name: string) =>
      name === 'app_redemption_totals'
        ? { data: [{ day: '2026-09-29', vouchers: 2, amount: 10000 }], error: null }
        : { data: [{ voucher_id: voucherId }], error: null },
    )
    const { queryClient, wrapper } = client()
    const { result } = renderHook(() => hooks.useRedemptions('2026-09-22', '2026-09-29'), { wrapper })

    await waitFor(() => expect(result.current.data?.log).toHaveLength(1))
    expect(result.current.data?.totals[0]).toMatchObject({ vouchers: 2 })
    expect(rpc).toHaveBeenCalledWith('app_redemption_totals', { p_from: '2026-09-22', p_to: '2026-09-29' })
    expect(rpc).toHaveBeenCalledWith('app_redemption_log', { p_from: '2026-09-22', p_to: '2026-09-29' })
    expect(queryClient.getQueryData(queryKeys.redemptionLog('2026-09-22', '2026-09-29'))).toBeDefined()
  })

  test('a failed read is an error', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'only ops' } })
    await expect(hooks.fetchRedemptions('2026-09-22', '2026-09-29')).rejects.toThrow('only ops')
  })
})
