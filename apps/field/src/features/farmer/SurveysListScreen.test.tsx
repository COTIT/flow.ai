import { render, screen, within } from '@testing-library/react'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSurveyList = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  Link: ({
    children,
    to,
    params,
    ...rest
  }: {
    children: ReactNode
    to: string
    params?: Record<string, string>
  } & Record<string, unknown>) => (
    <a href={to} data-to={to} data-params={JSON.stringify(params ?? {})} {...rest}>
      {children}
    </a>
  ),
}))
vi.mock('@/features/farmer/useSurveys', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/features/farmer/useSurveys')>()),
  useSurveyList: () => useSurveyList(),
}))

const { SurveysListScreen } = await import('@/features/farmer/SurveysListScreen')
const i18n = (await import('@/i18n')).default

const S1 = 'f1000000-0000-4000-8000-000000000001'
const S2 = 'f1000000-0000-4000-8000-000000000002'
const S3 = 'f1000000-0000-4000-8000-000000000003'
const VOUCHER = '51000000-0000-4000-8000-000000000001'

const survey = (id: string, over: Record<string, unknown> = {}) => ({
  id,
  title_en: 'Maize storage — DEMO',
  title_sw: null,
  description_en: 'How you store maize after harvest.',
  description_sw: null,
  reward_amount: 5000,
  currency: 'TZS',
  closes_at: '2099-10-31T09:00:00Z',
  status: 'live',
  questions: [{ id: 'q1' }, { id: 'q2' }, { id: 'q3' }],
  ...over,
})

const voucher = (over: Record<string, unknown> = {}) => ({
  id: VOUCHER,
  status: 'issued',
  expires_at: '2099-01-01T00:00:00Z',
  ...over,
})

const newRow = { survey: survey(S1), state: 'new', reason: null, voucherId: null, voucher: null }
const answeredRow = {
  survey: survey(S2, { title_en: 'Household energy use — DEMO' }),
  state: 'answered',
  reason: null,
  voucherId: VOUCHER,
  voucher: voucher(),
}
const unavailableRow = {
  survey: survey(S3, { title_en: 'Irrigation plans — DEMO', status: 'closed' }),
  state: 'unavailable',
  reason: 'your household has not been verified yet',
  voucherId: null,
  voucher: null,
}

function given(data: unknown[] | undefined, over: Record<string, unknown> = {}) {
  useSurveyList.mockReturnValue({ data, isLoading: false, error: null, refetch: vi.fn(), ...over })
}

beforeEach(async () => {
  useSurveyList.mockReset()
  await i18n.changeLanguage('en')
})

describe('SurveysListScreen states', () => {
  test('loading is a loading state, not an empty message', () => {
    given(undefined, { isLoading: true })
    render(<SurveysListScreen />)

    expect(screen.getByTestId('surveys-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    given(undefined, { error: new Error('could not reach the database') })
    render(<SurveysListScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByTestId('error-retry')).toBeInTheDocument()
  })

  // Zero rows is an answer: nothing to answer right now.
  test('no surveys is a real message, not an error', () => {
    given([])
    render(<SurveysListScreen />)

    expect(screen.getByTestId('empty-state')).toHaveTextContent('No surveys right now')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('has a title and says what a survey earns the household, once', () => {
    given([])
    render(<SurveysListScreen />)

    expect(screen.getByRole('heading', { level: 1, name: 'Surveys' })).toBeInTheDocument()
    expect(screen.getByTestId('surveys-list')).toHaveTextContent(/fixed cash incentive, once per survey/i)
  })
})

describe('a survey card', () => {
  test('one card per row, in the order given', () => {
    given([newRow, answeredRow, unavailableRow])
    render(<SurveysListScreen />)

    const cards = screen.getAllByTestId('survey-card')
    expect(cards.map((card) => card.getAttribute('data-state'))).toEqual(['new', 'answered', 'unavailable'])
  })

  test('shows the title, description, question count and closing date', () => {
    given([newRow])
    render(<SurveysListScreen />)

    const card = screen.getByTestId('survey-card')
    expect(card).toHaveTextContent('Maize storage — DEMO')
    expect(card).toHaveTextContent('How you store maize after harvest.')
    expect(card).toHaveTextContent('3 questions')
    expect(card).toHaveTextContent('Closes 31 Oct 2099, 12:00')
  })

  test('one question is singular', () => {
    given([{ ...newRow, survey: survey(S1, { questions: [{ id: 'q1' }] }) }])
    render(<SurveysListScreen />)

    expect(screen.getByTestId('survey-card')).toHaveTextContent('1 question')
  })

  test('states the incentive as a fixed cash amount, paid at the office', () => {
    given([newRow])
    render(<SurveysListScreen />)

    const incentive = screen.getByTestId('survey-incentive')
    expect(incentive).toHaveTextContent('Incentive')
    expect(incentive).toHaveTextContent('TZS 5,000.00')
    expect(incentive).toHaveTextContent('Paid in cash at the Ruaha office')
  })

  test('a survey with no closing date says nothing about closing', () => {
    given([{ ...newRow, survey: survey(S1, { closes_at: null }) }])
    render(<SurveysListScreen />)

    expect(screen.getByTestId('survey-card')).not.toHaveTextContent(/closes/i)
  })

  test('a new survey is marked new and links to its form', () => {
    given([newRow])
    render(<SurveysListScreen />)

    const card = screen.getByTestId('survey-card')
    expect(card).toHaveTextContent('New')
    const link = within(card).getByTestId('survey-open')
    expect(link).toHaveTextContent('Answer survey')
    expect(link).toHaveAttribute('data-to', '/farm/surveys/$surveyId')
    expect(link).toHaveAttribute('data-params', JSON.stringify({ surveyId: S1 }))
  })

  test('an answered survey shows its voucher status and links to the voucher', () => {
    given([answeredRow])
    render(<SurveysListScreen />)

    const card = screen.getByTestId('survey-card')
    expect(card).toHaveTextContent('Answered')
    expect(within(card).getByTestId('status-pill')).toHaveTextContent('Not yet collected')
    const link = within(card).getByTestId('survey-view-voucher')
    expect(link).toHaveTextContent('View voucher')
    expect(link).toHaveAttribute('data-params', JSON.stringify({ surveyId: S2 }))
    expect(within(card).queryByTestId('survey-open')).not.toBeInTheDocument()
  })

  // `expired` is derived from the date; the row still says `issued`.
  test('an answered survey whose voucher lapsed reads as expired', () => {
    given([{ ...answeredRow, voucher: voucher({ expires_at: '2020-01-01T00:00:00Z' }) }])
    render(<SurveysListScreen />)

    expect(screen.getByTestId('status-pill')).toHaveTextContent('Expired')
  })

  test('an answered survey whose voucher row has not arrived shows no pill', () => {
    given([{ ...answeredRow, voucher: null }])
    render(<SurveysListScreen />)

    expect(screen.queryByTestId('status-pill')).not.toBeInTheDocument()
    expect(screen.getByTestId('survey-view-voucher')).toBeInTheDocument()
  })

  // The reason is the database's sentence, shown as written — the same
  // function decides the list and the submit, so they cannot disagree.
  test('a survey that cannot be answered gives the database reason and no link', () => {
    given([unavailableRow])
    render(<SurveysListScreen />)

    const card = screen.getByTestId('survey-card')
    expect(card).toHaveTextContent('Not available')
    expect(within(card).getByTestId('survey-reason')).toHaveTextContent('Your household has not been verified yet')
    expect(within(card).queryByRole('link')).not.toBeInTheDocument()
  })

  test('titles follow the language, falling back to English when Swahili was never written', async () => {
    given([
      { ...newRow, survey: survey(S1, { title_sw: 'Uhifadhi wa mahindi', description_sw: null }) },
      { ...answeredRow, survey: survey(S2, { title_en: 'Energy', title_sw: null }) },
    ])
    await i18n.changeLanguage('sw')
    render(<SurveysListScreen />)

    const [first, second] = screen.getAllByTestId('survey-card')
    expect(first).toHaveTextContent('Uhifadhi wa mahindi')
    expect(first).toHaveTextContent('How you store maize after harvest.')
    expect(second).toHaveTextContent('Energy')
  })

  test('never calls the incentive earnings, a wallet, a balance or a payment, nor indicative', () => {
    given([newRow, answeredRow, unavailableRow])
    render(<SurveysListScreen />)

    expect(screen.getByTestId('surveys-list')).not.toHaveTextContent(/earning|wallet|balance|payment|indicative/i)
  })
})
