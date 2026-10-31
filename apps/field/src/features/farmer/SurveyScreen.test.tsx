import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSession = vi.fn()
const useSurveyEligibility = vi.fn()
const useSurvey = vi.fn()
const mutateAsync = vi.fn()

const SURVEY = 'f1000000-0000-4000-8000-000000000001'
const VOUCHER = '51000000-0000-4000-8000-000000000001'

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ surveyId: SURVEY }) }),
  Link: ({ children, to, params: _params, ...rest }: { children: ReactNode; to: string; params?: unknown } & Record<string, unknown>) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}))
vi.mock('@/app/session', () => ({ useSession: () => useSession() }))
// In-memory in place of IndexedDB, so a stored draft can be inspected.
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})
vi.mock('@/features/farmer/useSurveyEligibility', () => ({
  useSurveyEligibility: () => useSurveyEligibility(),
}))
const submitState: { isPending: boolean; error: Error | null; data: unknown } = {
  isPending: false,
  error: null,
  data: undefined,
}
vi.mock('@/features/farmer/useSurveys', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/farmer/useSurveys')>()),
  useSurvey: (id: string) => useSurvey(id),
  // No `mutate`: its per-call callbacks are skipped after unmount, so the
  // screen must chain the draft's finish on mutateAsync instead.
  useSubmitSurvey: () => ({ mutateAsync, reset: vi.fn(), ...submitState }),
}))
vi.mock('@/features/farmer/VoucherCard', () => ({
  VoucherCard: ({ voucherId }: { voucherId: string }) => (
    <div data-testid="voucher-card" data-voucher-id={voucherId} />
  ),
}))

const { SurveyScreen } = await import('@/features/farmer/SurveyScreen')
const { indexedDbDraftStore } = await import('@/lib/drafts')
const i18n = (await import('@/i18n')).default

const KEY = `form:u1:${SURVEY}:survey-answers`

const questions = [
  {
    id: 'q1',
    position: 1,
    kind: 'single_choice',
    prompt_en: 'Where do you store maize?',
    prompt_sw: 'Unahifadhi mahindi wapi?',
    required: true,
    options: [
      { value: 'house', label_en: 'In the house', label_sw: null },
      { value: 'crib', label_en: 'In a crib', label_sw: 'Kichanja' },
    ],
  },
  {
    id: 'q2',
    position: 2,
    kind: 'multi_choice',
    prompt_en: 'Which pests do you see?',
    prompt_sw: null,
    required: false,
    options: [
      { value: 'weevil', label_en: 'Weevils', label_sw: null },
      { value: 'rat', label_en: 'Rats', label_sw: null },
    ],
  },
  { id: 'q3', position: 3, kind: 'number', prompt_en: 'How many bags last season?', prompt_sw: null, required: true, options: [] },
  { id: 'q4', position: 4, kind: 'text', prompt_en: 'Anything else?', prompt_sw: null, required: false, options: [] },
  { id: 'q5', position: 5, kind: 'yes_no', prompt_en: 'Do you use hermetic bags?', prompt_sw: null, required: true, options: [] },
]

const survey = (over: Record<string, unknown> = {}) => ({
  id: SURVEY,
  title_en: 'Maize storage — DEMO',
  title_sw: null,
  description_en: 'How you store maize after harvest.',
  description_sw: null,
  reward_amount: 5000,
  currency: 'TZS',
  closes_at: '2099-10-31T09:00:00Z',
  status: 'live',
  questions,
  ...over,
})

const row = (over: Record<string, unknown> = {}) => ({
  survey_id: SURVEY,
  eligible: true,
  reason: null,
  response_id: null,
  voucher_id: null,
  ...over,
})

function given({ eligibility = [row()] as unknown[], data = survey() as unknown } = {}) {
  useSurveyEligibility.mockReturnValue({ isLoading: false, error: null, data: eligibility, refetch: vi.fn() })
  useSurvey.mockReturnValue({ isLoading: false, error: null, data, refetch: vi.fn() })
}

beforeEach(async () => {
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue({ response_id: 'r1', voucher_id: VOUCHER, code: 'K7QXM2PA9D' })
  submitState.isPending = false
  submitState.error = null
  submitState.data = undefined
  useSession.mockReturnValue({ isLoading: false, error: null, data: { userId: 'u1', appUser: { id: 'u1' }, memberships: [] } })
  await indexedDbDraftStore.clear(KEY)
  await i18n.changeLanguage('en')
  given()
})

const submitButton = () => screen.getByTestId('survey-submit')
const ready = () => waitFor(() => expect(submitButton()).toBeEnabled())

describe('SurveyScreen states', () => {
  test('asks for the survey in the route', () => {
    render(<SurveyScreen />)
    expect(useSurvey).toHaveBeenCalledWith(SURVEY)
  })

  test('loading is a loading state', () => {
    useSurvey.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<SurveyScreen />)

    expect(screen.getByTestId('survey-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('eligibility still loading is loading too', () => {
    useSurveyEligibility.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<SurveyScreen />)

    expect(screen.getByTestId('survey-loading')).toBeInTheDocument()
  })

  test('a failed read is an error, offering retry', () => {
    useSurvey.mockReturnValue({ isLoading: false, error: new Error('survey read failed'), data: undefined, refetch: vi.fn() })
    render(<SurveyScreen />)

    expect(screen.getByTestId('error-state')).toHaveTextContent('survey read failed')
    expect(screen.getByTestId('error-retry')).toBeInTheDocument()
  })

  // Zero rows is an answer: RLS says this survey is not visible.
  test('a survey that is not visible is not found, not an error', () => {
    given({ data: null })
    render(<SurveyScreen />)

    expect(screen.getByTestId('empty-state')).toHaveTextContent('This survey is not available.')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('a survey the eligibility answer does not name is not found either', () => {
    given({ eligibility: [] })
    render(<SurveyScreen />)

    expect(screen.getByTestId('empty-state')).toHaveTextContent('This survey is not available.')
    expect(screen.queryByTestId('survey-form')).not.toBeInTheDocument()
  })
})

describe('a survey the household has answered', () => {
  test('shows its voucher, and no form', () => {
    given({ eligibility: [row({ eligible: false, response_id: 'r1', voucher_id: VOUCHER })] })
    render(<SurveyScreen />)

    expect(screen.getByTestId('voucher-card')).toHaveAttribute('data-voucher-id', VOUCHER)
    expect(screen.queryByTestId('survey-form')).not.toBeInTheDocument()
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Maize storage — DEMO')
  })
})

describe('a survey the household cannot answer', () => {
  // The reason is the database's sentence — the same function the submit
  // runs — shown as written.
  test('gives the database reason and offers no form', () => {
    given({ eligibility: [row({ eligible: false, reason: 'set your own password before answering surveys' })] })
    render(<SurveyScreen />)

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Maize storage — DEMO')
    expect(screen.getByTestId('survey-blocked')).toHaveTextContent('Set your own password before answering surveys')
    expect(screen.queryByTestId('survey-form')).not.toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })
})

describe('the answer form', () => {
  test('states the incentive and that it is paid in cash at the office', () => {
    render(<SurveyScreen />)

    const incentive = screen.getByTestId('survey-incentive')
    expect(incentive).toHaveTextContent('TZS 5,000.00')
    expect(incentive).toHaveTextContent('Paid in cash at the Ruaha office')
  })

  test('one field per question, by kind', () => {
    render(<SurveyScreen />)

    const fields = screen.getAllByTestId('survey-question')
    expect(fields.map((f) => f.getAttribute('data-kind'))).toEqual(['single_choice', 'multi_choice', 'number', 'text', 'yes_no'])

    const [single, multi, number, text, yesNo] = fields
    expect(within(single).getAllByRole('radio').map((r) => r.getAttribute('value'))).toEqual(['house', 'crib'])
    expect(within(multi).getAllByRole('checkbox')).toHaveLength(2)
    expect(within(number).getByRole('textbox')).toHaveAttribute('inputmode', 'decimal')
    expect(within(text).getByRole('textbox').tagName).toBe('TEXTAREA')
    expect(within(yesNo).getByRole('radio', { name: 'Yes' })).toBeInTheDocument()
    expect(within(yesNo).getByRole('radio', { name: 'No' })).toBeInTheDocument()
  })

  // "question 3 is required" names a position; the form shows the same one.
  test('numbers each question by its position and marks it required or optional', () => {
    render(<SurveyScreen />)

    const [single, multi] = screen.getAllByTestId('survey-question')
    expect(single).toHaveTextContent(/^1\./)
    expect(single).toHaveTextContent('Required')
    expect(multi).toHaveTextContent('Optional')
  })

  test('each group is named by its question', () => {
    render(<SurveyScreen />)

    expect(screen.getByRole('group', { name: /Where do you store maize\?/ })).toBeInTheDocument()
    expect(screen.getByRole('textbox', { name: /How many bags last season\?/ })).toBeInTheDocument()
  })

  test('says the household can answer once', () => {
    render(<SurveyScreen />)
    expect(screen.getByTestId('survey-form')).toHaveTextContent(/can answer this survey once/i)
  })

  test('sends every answer in the shape its kind takes, with the draft reference', async () => {
    render(<SurveyScreen />)
    await ready()

    fireEvent.click(screen.getByRole('radio', { name: 'In a crib' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'Weevils' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'Rats' }))
    fireEvent.change(screen.getByRole('textbox', { name: /How many bags/ }), { target: { value: '12' } })
    fireEvent.change(screen.getByRole('textbox', { name: /Anything else/ }), { target: { value: 'We use a crib' } })
    fireEvent.click(screen.getByRole('radio', { name: 'No' }))
    fireEvent.click(submitButton())

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    const [input] = mutateAsync.mock.calls[0]
    expect(input.answers).toEqual({ q1: 'crib', q2: ['weevil', 'rat'], q3: '12', q4: 'We use a crib', q5: false })
    expect(input.clientRef).toMatch(/^[0-9a-f-]{36}$/)
  })

  test('unticking a choice takes it back out', async () => {
    render(<SurveyScreen />)
    await ready()

    fireEvent.click(screen.getByRole('checkbox', { name: 'Weevils' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'Rats' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'Weevils' }))
    expect(screen.getByRole('checkbox', { name: 'Weevils' })).not.toBeChecked()
    fireEvent.click(submitButton())

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0].answers.q2).toEqual(['rat'])
  })

  // Whether a required question may be left blank is the database's call,
  // and its refusal names the question.
  test('does not pre-validate: a blank required answer still goes to the database', async () => {
    render(<SurveyScreen />)
    await ready()
    fireEvent.click(submitButton())

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0].answers).toEqual({})
  })

  test('a refusal is shown in the database words, as an alert', () => {
    submitState.error = new Error('question 3 is required')
    render(<SurveyScreen />)

    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent('Question 3 is required')
    expect(alert).not.toHaveTextContent('Something went wrong')
  })

  test('a lost connection is said plainly, not as a browser message', () => {
    submitState.error = new TypeError('Failed to fetch')
    render(<SurveyScreen />)

    expect(screen.getByRole('alert')).toHaveTextContent(/could not reach the server/i)
  })

  test('in flight: the control is disabled and says so', () => {
    submitState.isPending = true
    render(<SurveyScreen />)

    expect(submitButton()).toBeDisabled()
    expect(submitButton()).toHaveTextContent('Submitting…')
  })

  test('a second tap in the same tick does not answer twice', async () => {
    let resolve!: (value: unknown) => void
    mutateAsync.mockReturnValue(new Promise((r) => { resolve = r }))
    render(<SurveyScreen />)
    await ready()

    fireEvent.click(submitButton())
    fireEvent.click(submitButton())
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    await act(async () => resolve({ voucher_id: VOUCHER }))
    expect(mutateAsync).toHaveBeenCalledTimes(1)
  })

  test('once saved, the voucher replaces the form', () => {
    submitState.data = { response_id: 'r1', voucher_id: VOUCHER, code: 'K7QXM2PA9D', replayed: false }
    render(<SurveyScreen />)

    expect(screen.getByTestId('voucher-card')).toHaveAttribute('data-voucher-id', VOUCHER)
    expect(screen.queryByTestId('survey-form')).not.toBeInTheDocument()
  })

  test('prompts and options follow the language, with English where Swahili was never written', async () => {
    await i18n.changeLanguage('sw')
    render(<SurveyScreen />)

    expect(screen.getByText(/Unahifadhi mahindi wapi\?/)).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'Kichanja' })).toBeInTheDocument()
    expect(screen.getByRole('radio', { name: 'In the house' })).toBeInTheDocument()
    expect(screen.getByText(/Which pests do you see\?/)).toBeInTheDocument()
  })

  test('never calls the incentive earnings, a wallet, a balance or a payment, nor indicative', () => {
    render(<SurveyScreen />)
    expect(screen.getByTestId('survey-screen')).not.toHaveTextContent(/earning|wallet|balance|payment|indicative/i)
  })
})

/**
 * Business-rules §12: a draft persists on change and clears only after a
 * confirmed server write. For a survey the draft also carries the client
 * reference, so a submit retried after a reload replays the first save's
 * voucher instead of being refused as a second answer.
 */
describe('the draft', () => {
  test('survives a reload', async () => {
    const first = render(<SurveyScreen />)
    await ready()
    fireEvent.change(screen.getByRole('textbox', { name: /How many bags/ }), { target: { value: '7' } })
    fireEvent.click(screen.getByRole('radio', { name: 'In a crib' }))
    await waitFor(async () => expect(await indexedDbDraftStore.get(KEY)).toMatchObject({ values: { q1: 'crib', q3: '7' } }))
    first.unmount()

    render(<SurveyScreen />)
    await waitFor(() => expect(screen.getByRole('textbox', { name: /How many bags/ })).toHaveValue('7'))
    expect(screen.getByRole('radio', { name: 'In a crib' })).toBeChecked()
  })

  test('a submit after a reload carries the same client reference', async () => {
    const first = render(<SurveyScreen />)
    await ready()
    fireEvent.click(screen.getByRole('radio', { name: 'Yes' }))
    await waitFor(async () => expect(await indexedDbDraftStore.get(KEY)).toBeDefined())
    const stored = (await indexedDbDraftStore.get(KEY)) as { clientRef: string }
    first.unmount()

    render(<SurveyScreen />)
    await waitFor(() => expect(screen.getByRole('radio', { name: 'Yes' })).toBeChecked())
    fireEvent.click(submitButton())

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0].clientRef).toBe(stored.clientRef)
  })

  test('a failed submit keeps the draft and retries with the same reference', async () => {
    mutateAsync.mockRejectedValueOnce(new TypeError('Failed to fetch'))
    render(<SurveyScreen />)
    await ready()
    fireEvent.click(screen.getByRole('radio', { name: 'Yes' }))
    await waitFor(async () => expect(await indexedDbDraftStore.get(KEY)).toBeDefined())

    fireEvent.click(submitButton())
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    await ready()
    // Not confirmed, so not cleared.
    expect(await indexedDbDraftStore.get(KEY)).toBeDefined()

    fireEvent.click(submitButton())
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(2))
    expect(mutateAsync.mock.calls[1][0].clientRef).toBe(mutateAsync.mock.calls[0][0].clientRef)
  })

  test('is cleared once the database confirms the save', async () => {
    render(<SurveyScreen />)
    await ready()
    fireEvent.click(screen.getByRole('radio', { name: 'Yes' }))
    await waitFor(async () => expect(await indexedDbDraftStore.get(KEY)).toBeDefined())

    fireEvent.click(submitButton())
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    await waitFor(async () => expect(await indexedDbDraftStore.get(KEY)).toBeUndefined())
  })

  test('shows that answers are not yet submitted once one is typed', async () => {
    render(<SurveyScreen />)
    await ready()
    fireEvent.click(screen.getByRole('radio', { name: 'Yes' }))

    expect(screen.getByTestId('survey-form')).toHaveTextContent('Not yet submitted')
  })
})
