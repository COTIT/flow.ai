import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const api = {
  farmSurveysList: vi.fn(),
  farmSurveysGet: vi.fn(),
  farmSurveysCreate: vi.fn(),
  farmSurveysUpdate: vi.fn(),
  farmSurveysPublish: vi.fn(),
  farmSurveysClose: vi.fn(),
  farmSurveysAddQuestion: vi.fn(),
  farmSurveysReplaceQuestion: vi.fn(),
  farmSurveysRemoveQuestion: vi.fn(),
  farmSurveysResults: vi.fn(),
  farmSurveysVouchers: vi.fn(),
  farmVouchersVoid: vi.fn(),
  currenciesList: vi.fn(),
}
const forward = (name: keyof typeof api) => (...args: unknown[]) => api[name](...args)

vi.mock('@genesy/api-client/endpoints/farm-surveys', () => ({
  farmSurveysList: forward('farmSurveysList'),
  farmSurveysGet: forward('farmSurveysGet'),
  farmSurveysCreate: forward('farmSurveysCreate'),
  farmSurveysUpdate: forward('farmSurveysUpdate'),
  farmSurveysPublish: forward('farmSurveysPublish'),
  farmSurveysClose: forward('farmSurveysClose'),
  farmSurveysAddQuestion: forward('farmSurveysAddQuestion'),
  farmSurveysReplaceQuestion: forward('farmSurveysReplaceQuestion'),
  farmSurveysRemoveQuestion: forward('farmSurveysRemoveQuestion'),
  farmSurveysResults: forward('farmSurveysResults'),
  farmSurveysVouchers: forward('farmSurveysVouchers'),
}))
vi.mock('@genesy/api-client/endpoints/farm-vouchers', () => ({ farmVouchersVoid: forward('farmVouchersVoid') }))
vi.mock('@genesy/api-client/endpoints/currencies', () => ({ currenciesList: forward('currenciesList') }))
vi.mock('@/app/workspace', () => ({ workspaceId: () => 'w1' }))

const hooks = await import('@/features/ops/surveys/useSurveyAdmin')
const { queryKeys } = await import('@/lib/queryKeys')

const surveyId = '30000000-0000-4000-8000-000000000001'
const voucherId = '40000000-0000-4000-8000-000000000001'

const page = (data: unknown[], current = 1, last = 1) => ({
  data,
  meta: { current_page: current, last_page: last, per_page: 100, from: 1, to: data.length, total: data.length },
})
const refusal = (status: number, message: string) => ({ isAxiosError: true, response: { status, data: { message } } })

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
  for (const fn of Object.values(api)) fn.mockReset()
})

describe('the survey list', () => {
  test('reads every page', async () => {
    api.farmSurveysList.mockResolvedValueOnce(page([{ id: 's1' }], 1, 2)).mockResolvedValueOnce(page([{ id: 's2' }], 2, 2))
    const rows = await hooks.fetchSurveyAdminList()
    expect(rows.map((r) => r.id)).toEqual(['s1', 's2'])
    expect(api.farmSurveysList).toHaveBeenLastCalledWith('w1', { page: 2, page_size: 100 })
  })

  test('is cached under the surveyAdmin key', async () => {
    api.farmSurveysList.mockResolvedValue(page([]))
    const { queryClient, wrapper } = client()
    const { result } = renderHook(() => hooks.useSurveyAdminList(), { wrapper })
    await waitFor(() => expect(result.current.isSuccess).toBe(true))
    expect(queryClient.getQueryData(queryKeys.surveyAdmin())).toEqual([])
  })
})

describe('one survey', () => {
  test('reads the survey with its questions in position order', async () => {
    api.farmSurveysGet.mockResolvedValue({
      data: { id: surveyId, questions: [{ id: 'q2', position: 2 }, { id: 'q1', position: 1 }] },
    })
    const detail = await hooks.fetchSurveyAdminDetail(surveyId)
    expect(api.farmSurveysGet).toHaveBeenCalledWith('w1', surveyId)
    expect(detail?.survey.id).toBe(surveyId)
    expect(detail?.questions.map((q) => q.id)).toEqual(['q1', 'q2'])
  })

  test('a 404 is not found, not an error', async () => {
    api.farmSurveysGet.mockRejectedValue(refusal(404, 'not found'))
    await expect(hooks.fetchSurveyAdminDetail(surveyId)).resolves.toBeNull()
  })

  test('a malformed id reaches not found without a request', async () => {
    await expect(hooks.fetchSurveyAdminDetail('nope')).resolves.toBeNull()
    expect(api.farmSurveysGet).not.toHaveBeenCalled()
  })

  test('any other failure rejects', async () => {
    const error = refusal(500, 'boom')
    api.farmSurveysGet.mockRejectedValue(error)
    await expect(hooks.fetchSurveyAdminDetail(surveyId)).rejects.toBe(error)
  })
})

describe('results and vouchers', () => {
  test("results are the API's figures, unchanged", async () => {
    const results = { responses: 3, issued: { count: 3, minor: 1500000 }, tally: [], numbers: [] }
    api.farmSurveysResults.mockResolvedValue({ data: results })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSurveyResults(surveyId), { wrapper })
    await waitFor(() => expect(result.current.data).toEqual(results))
    expect(api.farmSurveysResults).toHaveBeenCalledWith('w1', surveyId)
  })

  test('vouchers are read to the last page', async () => {
    api.farmSurveysVouchers
      .mockResolvedValueOnce(page([{ voucher_id: 'v1' }], 1, 2))
      .mockResolvedValueOnce(page([{ voucher_id: 'v2' }], 2, 2))
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSurveyVouchers(surveyId), { wrapper })
    await waitFor(() => expect(result.current.data).toHaveLength(2))
    expect(api.farmSurveysVouchers).toHaveBeenCalledWith('w1', surveyId, { page: 1, page_size: 100 })
  })

  test('currencies are the active ones of the workspace', async () => {
    api.currenciesList.mockResolvedValue(
      page([
        { id: 'c1', asset_type: 'tzs', decimals: 2, active: true },
        { id: 'c2', asset_type: 'usd', decimals: 2, active: false },
      ]),
    )
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useCurrencies(), { wrapper })
    await waitFor(() => expect(result.current.data?.map((c) => c.id)).toEqual(['c1']))
  })
})

describe('authoring', () => {
  const body = { title_en: 'Harvest', reward_minor: 500000, currency_id: 'c1' }

  test('a new survey is one POST; the created survey comes back', async () => {
    api.farmSurveysCreate.mockResolvedValue({ data: { id: surveyId } })
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useCreateSurvey(), { wrapper })
    await expect(result.current.mutateAsync(body)).resolves.toMatchObject({ id: surveyId })
    expect(api.farmSurveysCreate).toHaveBeenCalledTimes(1)
    expect(api.farmSurveysCreate).toHaveBeenCalledWith('w1', body)
    expect(invalidatedKeys(invalidate)).toContainEqual(queryKeys.surveyAdmin())
  })

  test('the refusal reaches the caller as the API wrote it', async () => {
    const error = refusal(403, 'only a farm admin may author surveys')
    api.farmSurveysCreate.mockRejectedValue(error)
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useCreateSurvey(), { wrapper })
    await expect(result.current.mutateAsync(body)).rejects.toBe(error)
  })

  test('saving a draft PATCHes it and refreshes every survey view', async () => {
    api.farmSurveysUpdate.mockResolvedValue({ data: { id: surveyId } })
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useUpdateSurvey(surveyId), { wrapper })
    await result.current.mutateAsync(body)
    expect(api.farmSurveysUpdate).toHaveBeenCalledWith('w1', surveyId, body)
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([queryKeys.surveyAdmin(), queryKeys.surveyAdminDetail(surveyId), queryKeys.surveyEligibility()]),
    )
  })

  test('publishing and closing are their own POSTs', async () => {
    api.farmSurveysPublish.mockResolvedValue({ data: {} })
    api.farmSurveysClose.mockResolvedValue({ data: {} })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSetSurveyStatus(surveyId), { wrapper })
    await result.current.mutateAsync('live')
    expect(api.farmSurveysPublish).toHaveBeenCalledWith('w1', surveyId)
    await result.current.mutateAsync('closed')
    expect(api.farmSurveysClose).toHaveBeenCalledWith('w1', surveyId)
  })
})

describe('questions', () => {
  const input = {
    position: 1,
    kind: 'single_choice' as const,
    prompt_en: ' Main crop? ',
    prompt_sw: ' ',
    required: true,
    options: [{ value: 'maize', label_en: 'Maize' }],
  }
  const sent = { ...input, prompt_en: 'Main crop?', prompt_sw: null }

  test('a new question is POSTed at the position given', async () => {
    api.farmSurveysAddQuestion.mockResolvedValue({ data: { id: 'q1' } })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSaveQuestion(surveyId), { wrapper })
    await expect(result.current.mutateAsync(input)).resolves.toEqual({ id: 'q1' })
    expect(api.farmSurveysAddQuestion).toHaveBeenCalledWith('w1', surveyId, sent)
  })

  test('an existing question is replaced in place', async () => {
    api.farmSurveysReplaceQuestion.mockResolvedValue(undefined)
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSaveQuestion(surveyId), { wrapper })
    await result.current.mutateAsync({ ...input, id: 'q1' })
    expect(api.farmSurveysReplaceQuestion).toHaveBeenCalledWith('w1', surveyId, 'q1', sent)
  })

  test("the draft-only refusal comes through in the API's words", async () => {
    const error = refusal(422, 'questions can only change while the survey is a draft')
    api.farmSurveysRemoveQuestion.mockRejectedValue(error)
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useRemoveQuestion(surveyId), { wrapper })
    await expect(result.current.mutateAsync('q1')).rejects.toBe(error)
    expect(api.farmSurveysRemoveQuestion).toHaveBeenCalledWith('w1', surveyId, 'q1')
  })

  test('moving swaps two positions, each question replaced whole', async () => {
    api.farmSurveysReplaceQuestion.mockResolvedValue(undefined)
    const q = (id: string, position: number) => ({
      id,
      position,
      kind: 'text',
      prompt_en: id,
      prompt_sw: null,
      required: false,
      options: [],
    })
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useSwapQuestions(surveyId), { wrapper })
    await result.current.mutateAsync({ first: q('a', 2), second: q('b', 1) })
    expect(api.farmSurveysReplaceQuestion.mock.calls).toEqual([
      ['w1', surveyId, 'a', { position: 1, kind: 'text', prompt_en: 'a', prompt_sw: null, required: false, options: [] }],
      ['w1', surveyId, 'b', { position: 2, kind: 'text', prompt_en: 'b', prompt_sw: null, required: false, options: [] }],
    ])
  })
})

describe('voiding a voucher', () => {
  test('POSTs the reason once and refreshes the vouchers, the trail, the list and the log', async () => {
    api.farmVouchersVoid.mockResolvedValue(undefined)
    const { wrapper, invalidate } = client()
    const { result } = renderHook(() => hooks.useVoidVoucher(surveyId), { wrapper })
    await result.current.mutateAsync({ voucherId, reason: 'duplicate household' })
    expect(api.farmVouchersVoid).toHaveBeenCalledTimes(1)
    expect(api.farmVouchersVoid).toHaveBeenCalledWith('w1', voucherId, { reason: 'duplicate household' })
    expect(invalidatedKeys(invalidate)).toEqual(
      expect.arrayContaining([
        queryKeys.surveyVouchers(surveyId),
        queryKeys.surveyTally(surveyId),
        queryKeys.voucherTimeline(voucherId),
        ['redemptionLog'],
      ]),
    )
  })

  test("a refusal is the API's sentence", async () => {
    const error = refusal(422, 'a reason is required')
    api.farmVouchersVoid.mockRejectedValue(error)
    const { wrapper } = client()
    const { result } = renderHook(() => hooks.useVoidVoucher(surveyId), { wrapper })
    await expect(result.current.mutateAsync({ voucherId, reason: '' })).rejects.toBe(error)
  })
})
