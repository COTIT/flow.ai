import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const farmSurveysGet = vi.fn()
const farmSurveysAnswer = vi.fn()
const farmSurveysEligibility = vi.fn()
const farmVouchersMine = vi.fn()

vi.mock('@genesy/api-client/endpoints/farm-surveys', () => ({
  farmSurveysGet: (...args: unknown[]) => farmSurveysGet(...args),
  farmSurveysAnswer: (...args: unknown[]) => farmSurveysAnswer(...args),
  farmSurveysEligibility: (...args: unknown[]) => farmSurveysEligibility(...args),
}))
vi.mock('@genesy/api-client/endpoints/farm-vouchers', () => ({
  farmVouchersMine: (...args: unknown[]) => farmVouchersMine(...args),
}))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

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
const REWARD = { minor: 500000, decimals: 2, currency: 'TZS', currency_id: 'c-tzs' }
const failed = (status: number, message: string) => ({ response: { status, data: { message } } })
const page = (data: unknown[]) => ({
  data,
  meta: { current_page: 1, last_page: 1, per_page: 100, from: 1, to: data.length, total: data.length },
})

/** GET /farm/surveys/{survey}: questions embedded and already in position order. */
const surveyRow = (over: Record<string, unknown> = {}) => ({
  id: SURVEY,
  title_en: 'Maize storage — DEMO',
  title_sw: null,
  description_en: 'How you store maize after harvest.',
  description_sw: null,
  reward: REWARD,
  closes_at: '2026-10-31T20:59:00Z',
  status: 'live',
  questions: [
    {
      id: 'q1',
      position: 1,
      kind: 'single_choice',
      prompt_en: 'Where do you store maize?',
      prompt_sw: null,
      required: true,
      options: [
        { value: 'house', label_en: 'In the house', label_sw: null },
        { value: 'crib', label_en: 'In a crib', label_sw: 'Kichanja' },
      ],
    },
    { id: 'q2', position: 2, kind: 'yes_no', prompt_en: 'Do you use bags?', prompt_sw: null, required: false, options: [] },
  ],
  ...over,
})

beforeEach(() => {
  farmSurveysGet.mockReset()
  farmSurveysAnswer.mockReset()
  farmSurveysEligibility.mockReset()
  farmVouchersMine.mockReset()
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
  test('reads the survey of this workspace, both languages, questions in the API order', async () => {
    farmSurveysGet.mockResolvedValue({ data: surveyRow() })
    const survey = await fetchSurvey(SURVEY)

    expect(farmSurveysGet).toHaveBeenCalledWith('w1', SURVEY)
    expect(survey?.questions.map((q) => q.id)).toEqual(['q1', 'q2'])
    expect(survey?.questions[0].options).toEqual([
      { value: 'house', label_en: 'In the house', label_sw: null },
      { value: 'crib', label_en: 'In a crib', label_sw: 'Kichanja' },
    ])
  })

  test('the reward is the API Money, integer minor units', async () => {
    farmSurveysGet.mockResolvedValue({ data: surveyRow() })
    expect((await fetchSurvey(SURVEY))?.reward).toEqual(REWARD)
  })

  // 404 is an answer: not visible to this household, or not a survey.
  test('a survey that is not visible is null', async () => {
    farmSurveysGet.mockRejectedValue(failed(404, 'not found'))
    await expect(fetchSurvey(SURVEY)).resolves.toBeNull()
  })

  test('a malformed id reaches "not found" without a round trip', async () => {
    await expect(fetchSurvey('not-a-uuid')).resolves.toBeNull()
    expect(farmSurveysGet).not.toHaveBeenCalled()
  })

  test('any other failure rejects', async () => {
    farmSurveysGet.mockRejectedValue(failed(500, 'survey read failed'))
    await expect(fetchSurvey(SURVEY)).rejects.toMatchObject({ response: { status: 500 } })
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
describe('the answers sent to POST farm/surveys/{survey}/responses', () => {
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
    amount: REWARD,
    expires_at: '2026-10-29T00:00:00Z',
    replayed: false,
  }

  test('posts the answers with the client reference', async () => {
    farmSurveysAnswer.mockResolvedValue({ data: result })
    await expect(submitSurvey(SURVEY, { q1: 'crib' }, CLIENT_REF)).resolves.toEqual(result)
    expect(farmSurveysAnswer).toHaveBeenCalledWith('w1', SURVEY, { answers: { q1: 'crib' }, client_ref: CLIENT_REF })
  })

  test('a refusal rejects with the API error, its sentence untouched', async () => {
    farmSurveysAnswer.mockRejectedValue(failed(422, 'question 1 is required'))
    await expect(submitSurvey(SURVEY, {}, CLIENT_REF)).rejects.toMatchObject({
      response: { data: { message: 'question 1 is required' } },
    })
  })

  test('success refreshes eligibility, vouchers and the trail', async () => {
    farmSurveysAnswer.mockResolvedValue({ data: result })
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const { result: hook } = renderHook(() => useSubmitSurvey(SURVEY), { wrapper: wrapperFor(queryClient) })

    await hook.current.mutateAsync({ answers: { q1: 'crib' }, clientRef: CLIENT_REF })

    const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey)
    expect(keys).toContainEqual(queryKeys.surveyEligibility())
    expect(keys).toContainEqual(queryKeys.farmerVouchers())
    expect(keys).toContainEqual(queryKeys.voucherTimeline(VOUCHER))
  })

  test('a failure invalidates nothing', async () => {
    farmSurveysAnswer.mockRejectedValue(failed(422, 'this survey is closed'))
    const queryClient = client()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const { result: hook } = renderHook(() => useSubmitSurvey(SURVEY), { wrapper: wrapperFor(queryClient) })

    await expect(hook.current.mutateAsync({ answers: {}, clientRef: CLIENT_REF })).rejects.toBeTruthy()
    expect(invalidate).not.toHaveBeenCalled()
  })
})

const survey = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  title_en: `Survey ${id.slice(-1)}`,
  title_sw: null,
  description_en: null,
  description_sw: null,
  reward: REWARD,
  closes_at: null,
  status: 'live',
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
    farmSurveysEligibility.mockResolvedValue(page([eligibility(SURVEY, { eligible: true })]))
    farmSurveysGet.mockResolvedValue({ data: surveyRow() })
    farmVouchersMine.mockResolvedValue(page([]))

    const { result } = renderHook(() => useSurveyList(), { wrapper: wrapperFor(client()) })
    await waitFor(() => expect(result.current.isLoading).toBe(false))

    expect(result.current.error).toBeNull()
    expect(result.current.data).toHaveLength(1)
    expect(result.current.data?.[0].survey.questions).toHaveLength(2)
    expect(result.current.data?.[0].state).toBe('new')
  })

  test('the hook reports a failed survey read as an error, not as no surveys', async () => {
    farmSurveysEligibility.mockResolvedValue(page([eligibility(SURVEY, { eligible: true })]))
    farmSurveysGet.mockRejectedValue(new Error('survey read failed'))
    farmVouchersMine.mockResolvedValue(page([]))

    const { result } = renderHook(() => useSurveyList(), { wrapper: wrapperFor(client()) })
    await waitFor(() => expect(result.current.error).not.toBeNull())
    expect(result.current.error?.message).toBe('survey read failed')
  })
})
