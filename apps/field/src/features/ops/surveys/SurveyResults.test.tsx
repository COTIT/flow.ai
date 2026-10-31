import { render, screen, within } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSurveyTally = vi.fn()
vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useSurveyTally: (id: string) => useSurveyTally(id),
}))

const { SurveyResults } = await import('@/features/ops/surveys/SurveyResults')
await import('@/i18n')

const SURVEY = '30000000-0000-4000-8000-000000000001'

const q = (over: Record<string, unknown>) => ({
  survey_id: SURVEY,
  prompt_sw: null,
  required: true,
  options: [],
  deleted_at: null,
  created_at: '2026-09-28T08:00:00Z',
  updated_at: '2026-09-28T08:00:00Z',
  ...over,
})

const questions = [
  q({
    id: 'q1',
    position: 1,
    kind: 'single_choice',
    prompt_en: 'Main crop?',
    options: [
      { value: 'maize', label_en: 'Maize' },
      { value: 'beans', label_en: 'Beans' },
    ],
  }),
  q({ id: 'q2', position: 2, kind: 'yes_no', prompt_en: 'Do you irrigate?' }),
  q({ id: 'q3', position: 3, kind: 'number', prompt_en: 'How many acres?' }),
  q({ id: 'q4', position: 4, kind: 'text', prompt_en: 'Anything else?' }),
]

const summary = {
  survey_id: SURVEY,
  project_id: 'p1',
  responses: 3,
  issued_count: 3,
  issued_amount: 15000,
  redeemed_count: 1,
  redeemed_amount: 5000,
  outstanding_count: 1,
  outstanding_amount: 5000,
  expired_count: 0,
  void_count: 1,
}

const tally = {
  choices: [
    { question_id: 'q1', option_value: 'maize', answer_count: 2 },
    { question_id: 'q1', option_value: 'sorghum', answer_count: 1 },
    { question_id: 'q2', option_value: 'true', answer_count: 2 },
    { question_id: 'q2', option_value: 'false', answer_count: 1 },
  ],
  numbers: [{ question_id: 'q3', answer_count: 3, average: 2.33, minimum: 1, maximum: 4 }],
}

const renderResults = (over: { summary?: unknown } = {}) =>
  render(
    <SurveyResults
      surveyId={SURVEY}
      currency="TZS"
      questions={questions as never}
      summary={('summary' in over ? over.summary : summary) as never}
    />,
  )

beforeEach(() => {
  useSurveyTally.mockReset()
  useSurveyTally.mockReturnValue({ isLoading: false, error: null, data: tally, refetch: vi.fn() })
})

describe('the summary figures', () => {
  test('are the view figures, each with its cash where it has one', () => {
    renderResults()

    expect(screen.getByTestId('summary-responses')).toHaveTextContent('3')
    expect(screen.getByTestId('summary-issued')).toHaveTextContent('3 · TZS 15,000.00')
    expect(screen.getByTestId('summary-redeemed')).toHaveTextContent('1 · TZS 5,000.00')
    expect(screen.getByTestId('summary-outstanding')).toHaveTextContent('1 · TZS 5,000.00')
    expect(screen.getByTestId('summary-expired')).toHaveTextContent('0')
    expect(screen.getByTestId('summary-void')).toHaveTextContent('1')
  })

  test('say what each figure is', () => {
    renderResults()
    const figures = screen.getByTestId('survey-summary')
    for (const label of ['Households answered', 'Issued', 'Collected', 'Not yet collected', 'Expired', 'Cancelled']) {
      expect(figures).toHaveTextContent(label)
    }
  })

  test('no summary row is absent figures, not zeros', () => {
    renderResults({ summary: null })
    expect(screen.getByTestId('summary-responses')).toHaveTextContent('—')
    expect(screen.getByTestId('summary-issued')).toHaveTextContent('—')
  })
})

describe('the answer tally', () => {
  test('every question sits inside one tally section', () => {
    renderResults()
    const tally = screen.getByTestId('survey-tally')
    for (const question of screen.getAllByTestId('tally-question')) expect(tally).toContainElement(question)
    // The figures above it are a different stop; the tally is the questions only.
    expect(tally).not.toContainElement(screen.getByTestId('survey-summary'))
  })

  test('asks for this survey', () => {
    renderResults()
    expect(useSurveyTally).toHaveBeenCalledWith(SURVEY)
  })

  test('choice counts carry the option labels from the question', () => {
    renderResults()
    const [choice] = screen.getAllByTestId('tally-question')
    const rows = within(choice).getAllByTestId('tally-option')

    expect(choice).toHaveTextContent('Main crop?')
    expect(rows[0]).toHaveTextContent('Maize')
    expect(rows[0]).toHaveTextContent('2')
    // An option nobody chose has no tally row: nobody chose it.
    expect(rows[1]).toHaveTextContent('Beans')
    expect(rows[1]).toHaveTextContent('0')
  })

  test('an answered value the question no longer lists is still shown', () => {
    renderResults()
    const [choice] = screen.getAllByTestId('tally-question')
    expect(within(choice).getAllByTestId('tally-option')[2]).toHaveTextContent('sorghum')
  })

  test('yes/no answers read as Yes and No', () => {
    renderResults()
    const rows = within(screen.getAllByTestId('tally-question')[1]).getAllByTestId('tally-option')
    expect(rows[0]).toHaveTextContent('Yes')
    expect(rows[0]).toHaveTextContent('2')
    expect(rows[1]).toHaveTextContent('No')
    expect(rows[1]).toHaveTextContent('1')
  })

  test('a number question shows the view average, lowest and highest', () => {
    renderResults()
    const number = within(screen.getAllByTestId('tally-question')[2])
    expect(number.getByTestId('tally-average')).toHaveTextContent('Average 2.33')
    expect(number.getByTestId('tally-minimum')).toHaveTextContent('Lowest 1')
    expect(number.getByTestId('tally-maximum')).toHaveTextContent('Highest 4')
    expect(number.getByTestId('tally-count')).toHaveTextContent('3 answers')
  })

  test('free text is not tallied', () => {
    renderResults()
    const text = within(screen.getAllByTestId('tally-question')[3])
    expect(text.getByTestId('tally-text')).toBeInTheDocument()
    expect(text.queryByTestId('tally-option')).not.toBeInTheDocument()
  })

  test('a failed tally read is an error, the summary still shows', () => {
    useSurveyTally.mockReturnValue({ isLoading: false, error: new Error('tally failed'), data: undefined, refetch: vi.fn() })
    renderResults()
    expect(screen.getByTestId('error-state')).toHaveTextContent('tally failed')
    expect(screen.getByTestId('summary-responses')).toHaveTextContent('3')
  })

  test('loading is a loading state', () => {
    useSurveyTally.mockReturnValue({ isLoading: true, error: null, data: undefined })
    renderResults()
    expect(screen.getByTestId('tally-loading')).toBeInTheDocument()
  })
})
