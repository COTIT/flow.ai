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
  const record = (method: string) =>
    vi.fn((...args: unknown[]) => {
      calls.push({ table, method, args })
      return chain
    })
  for (const method of ['select', 'eq', 'is', 'order', 'in']) chain[method] = record(method)
  const settle = () => Promise.resolve(responses.get(table) ?? { data: [], error: null })
  chain.maybeSingle = vi.fn(() => {
    calls.push({ table, method: 'maybeSingle', args: [] })
    return settle()
  })
  chain.then = (resolve: (value: unknown) => unknown, reject?: (reason: unknown) => unknown) =>
    settle().then(resolve, reject)
  return chain
})

vi.mock('@/lib/supabase', () => ({ supabase: { from, rpc } }))

const {
  buildSurveyList,
  fetchSurvey,
  submitSurvey,
  toSurveyAnswers,
  useSubmitSurvey,
  useSurveyList,
  readChoices,
  sentence,
  writeChoices,
} = await import('@/features/farmer/useSurveys')
const { queryKeys } = await import('@/lib/queryKeys')

const SURVEY = 'f1000000-0000-4000-8000-000000000001'
const SURVEY_2 = 'f1000000-0000-4000-8000-000000000002'
const SURVEY_3 = 'f1000000-0000-4000-8000-000000000003'
const VOUCHER = '51000000-0000-4000-8000-000000000001'
const CLIENT_REF = '99000000-0000-4000-8000-000000000001'

const surveyRow = (over: Record<string, unknown> = {}) => ({
  id: SURVEY,
  title_en: 'Maize storage — DEMO',
  title_sw: null,
  description_en: 'How you store maize after harvest.',
  description_sw: null,
  reward_amount: 5000,
  currency: 'TZS',
  closes_at: '2026-10-31T20:59:00Z',
  status: 'live',
  survey_question: [
    { id: 'q2', position: 2, kind: 'yes_no', prompt_en: 'Do you use bags?', prompt_sw: null, required: false, options: [] },
    {
      id: 'q1',
      position: 1,
      kind: 'single_choice',
      prompt_en: 'Where do you store maize?',
      prompt_sw: null,
      required: true,
      options: [
        { value: 'house', label_en: 'In the house' },
        { value: 'crib', label_en: 'In a crib', label_sw: 'Kichanja' },
      ],
    },
  ],
  ...over,
})

beforeEach(() => {
  responses.clear()
  calls.length = 0
  rpc.mockReset()
  from.mockClear()
})

function client() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
}

function wrapperFor(queryClient: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  )
}

describe('one survey and its questions', () => {
  test('asks for both languages and never chooses one', async () => {
    responses.set('survey', { data: surveyRow(), error: null })
    await fetchSurvey(SURVEY)

    const select = String(calls.find((c) => c.table === 'survey' && c.method === 'select')?.args[0])
    for (const column of ['title_en', 'title_sw', 'description_en', 'description_sw', 'prompt_en', 'prompt_sw', 'options']) {
      expect(select).toContain(column)
    }
  })

  test('leaves deleted questions out on the server', async () => {
    responses.set('survey', { data: surveyRow(), error: null })
    await fetchSurvey(SURVEY)

    expect(calls).toContainEqual({ table: 'survey', method: 'is', args: ['survey_question.deleted_at', null] })
  })

  test('returns the questions in position order', async () => {
    responses.set('survey', { data: surveyRow(), error: null })
    const survey = await fetchSurvey(SURVEY)
    expect(survey?.questions.map((q) => q.id)).toEqual(['q1', 'q2'])
  })

  test('keeps both option labels, with Swahili absent when it was never written', async () => {
    responses.set('survey', { data: surveyRow(), error: null })
    const survey = await fetchSurvey(SURVEY)
    expect(survey?.questions[0].options).toEqual([
      { value: 'house', label_en: 'In the house', label_sw: null },
      { value: 'crib', label_en: 'In a crib', label_sw: 'Kichanja' },
    ])
  })

  test('an option without a value is not offered', async () => {
    responses.set('survey', {
      data: surveyRow({
        survey_question: [
          { id: 'q1', position: 1, kind: 'single_choice', prompt_en: 'P', prompt_sw: null, required: true, options: [{ label_en: 'No value' }, 'junk', { value: 'a', label_en: 'A' }] },
        ],
      }),
      error: null,
    })
    const survey = await fetchSurvey(SURVEY)
    expect(survey?.questions[0].options.map((o) => o.value)).toEqual(['a'])
  })

  test('options that are not an array read as none', async () => {
    responses.set('survey', {
      data: surveyRow({
        survey_question: [{ id: 'q1', position: 1, kind: 'text', prompt_en: 'P', prompt_sw: null, required: false, options: null }],
      }),
      error: null,
    })
    const survey = await fetchSurvey(SURVEY)
    expect(survey?.questions[0].options).toEqual([])
  })

  test('the reward arrives as a number', async () => {
    responses.set('survey', { data: surveyRow({ reward_amount: '5000.00' }), error: null })
    expect((await fetchSurvey(SURVEY))?.reward_amount).toBe(5000)
  })

  // Zero rows is an answer: RLS says this survey is not visible.
  test('a survey that is not visible is null', async () => {
    responses.set('survey', { data: null, error: null })
    await expect(fetchSurvey(SURVEY)).resolves.toBeNull()
  })

  test('a malformed id reaches "not found" without a uuid parse failure', async () => {
    await expect(fetchSurvey('not-a-uuid')).resolves.toBeNull()
    expect(from).not.toHaveBeenCalled()
  })

  test('a failed read rejects', async () => {
    responses.set('survey', { data: null, error: { message: 'survey read failed' } })
    await expect(fetchSurvey(SURVEY)).rejects.toThrow('survey read failed')
  })
})

/**
 * The database's refusals are plain lowercase English sentences. Shown as
 * written, with only the first letter raised for display.
 */
describe('a database sentence on screen', () => {
  test('gains a capital and nothing else', () => {
    expect(sentence('your household has not been verified yet')).toBe('Your household has not been verified yet')
    expect(sentence('question 2: enter a number')).toBe('Question 2: enter a number')
  })

  test('leaves an empty string empty', () => {
    expect(sentence('')).toBe('')
  })
})

describe('multi-choice answers in a string-valued draft', () => {
  test('round-trip through the draft', () => {
    expect(readChoices(writeChoices(['a', 'b,c']))).toEqual(['a', 'b,c'])
  })

  test('an empty or damaged value reads as nothing chosen', () => {
    expect(readChoices('')).toEqual([])
    expect(readChoices('{not json')).toEqual([])
    expect(readChoices('"a"')).toEqual([])
    expect(readChoices('[1, "a"]')).toEqual(['a'])
  })
})

/**
 * `p_answers` is keyed by question id, one JSON shape per kind. Nothing here
 * decides whether an answer is acceptable — a missing required answer, an
 * unlisted option or text in a number field is the database's refusal to
 * make, in its own words.
 */
describe('the answers sent to app_survey_submit', () => {
  const q = (id: string, kind: string, required = true) => ({
    id,
    position: 1,
    kind: kind as never,
    prompt_en: 'P',
    prompt_sw: null,
    required,
    options: [],
  })

  test('one shape per kind', () => {
    const answers = toSurveyAnswers(
      [q('a', 'single_choice'), q('b', 'multi_choice'), q('c', 'number'), q('d', 'text'), q('e', 'yes_no'), q('f', 'yes_no')],
      { a: 'crib', b: writeChoices(['x', 'y']), c: '12.5', d: 'We use a crib', e: 'yes', f: 'no' },
    )
    expect(answers).toEqual({ a: 'crib', b: ['x', 'y'], c: '12.5', d: 'We use a crib', e: true, f: false })
  })

  test('unanswered questions are omitted, required or not', () => {
    const answers = toSurveyAnswers(
      [q('a', 'single_choice'), q('b', 'multi_choice', false), q('c', 'number', false), q('d', 'text'), q('e', 'yes_no')],
      { a: '', b: writeChoices([]), c: '  ', d: '   ', e: '' },
    )
    expect(answers).toEqual({})
  })

  // The database parses the number; a copy of its parser here would drift.
  test('a number is sent as typed, not pre-validated', () => {
    expect(toSurveyAnswers([q('c', 'number')], { c: 'abc' })).toEqual({ c: 'abc' })
  })

  test('a question the draft never saw is simply unanswered', () => {
    expect(toSurveyAnswers([q('a', 'text')], {})).toEqual({})
  })
})

describe('submitting a survey', () => {
  const result = {
    response_id: 'r1',
    voucher_id: VOUCHER,
    code: 'K7QXM2PA9D',
    amount: 5000,
    currency: 'TZS',
    expires_at: '2026-10-29T00:00:00Z',
    replayed: false,
  }

  test('calls the RPC with the survey, the answers and the client reference', async () => {
    rpc.mockResolvedValue({ data: result, error: null })
    await expect(submitSurvey(SURVEY, { q1: 'crib' }, CLIENT_REF)).resolves.toEqual(result)
    expect(rpc).toHaveBeenCalledWith('app_survey_submit', {
      p_survey_id: SURVEY,
      p_answers: { q1: 'crib' },
      p_client_ref: CLIENT_REF,
    })
  })

  test('a refusal rejects with the database sentence, word for word', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'question 1 is required' } })
    await expect(submitSurvey(SURVEY, {}, CLIENT_REF)).rejects.toThrow(/^question 1 is required$/)
  })

  test('success keeps the code and refreshes eligibility, vouchers and the trail', async () => {
    rpc.mockResolvedValue({ data: result, error: null })
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const { result: hook } = renderHook(() => useSubmitSurvey(SURVEY), { wrapper: wrapperFor(queryClient) })

    await hook.current.mutateAsync({ answers: { q1: 'crib' }, clientRef: CLIENT_REF })

    expect(queryClient.getQueryData(queryKeys.voucherCode(VOUCHER))).toBe('K7QXM2PA9D')
    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toContainEqual(queryKeys.surveyEligibility())
    expect(keys).toContainEqual(queryKeys.farmerVouchers())
    expect(keys).toContainEqual(queryKeys.voucherTimeline(VOUCHER))
  })

  test('a failure invalidates nothing', async () => {
    rpc.mockResolvedValue({ data: null, error: { message: 'this survey is closed' } })
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const { result: hook } = renderHook(() => useSubmitSurvey(SURVEY), { wrapper: wrapperFor(queryClient) })

    await expect(hook.current.mutateAsync({ answers: {}, clientRef: CLIENT_REF })).rejects.toThrow('this survey is closed')
    expect(invalidate).not.toHaveBeenCalled()
  })
})

const survey = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  title_en: `Survey ${id.slice(-1)}`,
  title_sw: null,
  description_en: null,
  description_sw: null,
  reward_amount: 5000,
  currency: 'TZS',
  closes_at: null,
  status: 'live' as const,
  questions: [],
  ...over,
})

const eligibility = (survey_id: string, over: Record<string, unknown> = {}) => ({
  survey_id,
  eligible: false,
  reason: null,
  response_id: null,
  voucher_id: null,
  ...over,
})

describe('the list a farmer sees', () => {
  test('new first, then answered, then not available', () => {
    const rows = buildSurveyList(
      [
        eligibility(SURVEY, { reason: 'this survey is closed' }),
        eligibility(SURVEY_2, { response_id: 'r1', voucher_id: VOUCHER }),
        eligibility(SURVEY_3, { eligible: true }),
      ],
      new Map([
        [SURVEY, survey(SURVEY)],
        [SURVEY_2, survey(SURVEY_2)],
        [SURVEY_3, survey(SURVEY_3)],
      ]),
      [],
    )
    expect(rows.map((r) => [r.survey.id, r.state])).toEqual([
      [SURVEY_3, 'new'],
      [SURVEY_2, 'answered'],
      [SURVEY, 'unavailable'],
    ])
  })

  test('within a group, the survey closing soonest comes first and no date comes last', () => {
    const rows = buildSurveyList(
      [eligibility(SURVEY, { eligible: true }), eligibility(SURVEY_2, { eligible: true }), eligibility(SURVEY_3, { eligible: true })],
      new Map([
        [SURVEY, survey(SURVEY)],
        [SURVEY_2, survey(SURVEY_2, { closes_at: '2026-11-30T00:00:00Z' })],
        [SURVEY_3, survey(SURVEY_3, { closes_at: '2026-10-15T00:00:00Z' })],
      ]),
      [],
    )
    expect(rows.map((r) => r.survey.id)).toEqual([SURVEY_3, SURVEY_2, SURVEY])
  })

  test('the database reason travels with a survey that cannot be answered', () => {
    const [row] = buildSurveyList(
      [eligibility(SURVEY, { reason: 'your household has not been verified yet' })],
      new Map([[SURVEY, survey(SURVEY)]]),
      [],
    )
    expect(row.reason).toBe('your household has not been verified yet')
  })

  test('an answered survey carries its voucher row', () => {
    const voucher = { id: VOUCHER, status: 'issued', expires_at: '2026-10-20T00:00:00Z' }
    const [row] = buildSurveyList(
      [eligibility(SURVEY, { response_id: 'r1', voucher_id: VOUCHER })],
      new Map([[SURVEY, survey(SURVEY)]]),
      [voucher as never],
    )
    expect(row.voucherId).toBe(VOUCHER)
    expect(row.voucher?.id).toBe(VOUCHER)
  })

  // The eligibility RPC can name a survey whose row RLS does not show. Zero
  // rows is an answer: it is not listed.
  test('a survey whose row is not visible is left out', () => {
    const rows = buildSurveyList(
      [eligibility(SURVEY, { eligible: true }), eligibility(SURVEY_2, { eligible: true })],
      new Map([[SURVEY, survey(SURVEY)], [SURVEY_2, null]]),
      [],
    )
    expect(rows.map((r) => r.survey.id)).toEqual([SURVEY])
  })

  test('the hook joins eligibility, surveys and vouchers', async () => {
    rpc.mockImplementation(async (name: string) =>
      name === 'app_survey_eligibility'
        ? { data: [eligibility(SURVEY, { eligible: true })], error: null }
        : { data: null, error: null },
    )
    responses.set('survey', { data: surveyRow(), error: null })
    responses.set('survey_voucher', { data: [], error: null })

    const { result } = renderHook(() => useSurveyList(), { wrapper: wrapperFor(client()) })
    await waitFor(() => expect(result.current.isLoading).toBe(false))

    expect(result.current.error).toBeNull()
    expect(result.current.data).toHaveLength(1)
    expect(result.current.data?.[0].survey.questions).toHaveLength(2)
    expect(result.current.data?.[0].state).toBe('new')
  })

  test('the hook reports a failed survey read as an error, not as no surveys', async () => {
    rpc.mockResolvedValue({ data: [eligibility(SURVEY, { eligible: true })], error: null })
    responses.set('survey', { data: null, error: { message: 'survey read failed' } })

    const { result } = renderHook(() => useSurveyList(), { wrapper: wrapperFor(client()) })
    await waitFor(() => expect(result.current.error).not.toBeNull())
    expect(result.current.error?.message).toBe('survey read failed')
  })
})
