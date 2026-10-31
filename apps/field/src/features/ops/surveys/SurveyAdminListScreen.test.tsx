import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

const useSurveyAdminList = vi.fn()
const mutateAsync = vi.fn()
const createState = { isPending: false, isError: false, error: null as Error | null }
const navigate = vi.fn()

vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useSurveyAdminList: () => useSurveyAdminList(),
  // No `mutate`: its per-call callbacks are skipped after unmount.
  useCreateSurvey: () => ({ mutateAsync, reset: vi.fn(), ...createState }),
}))
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigate,
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))

const PROJECT = '20000000-0000-4000-8000-000000000001'
const session = {
  data: {
    userId: undefined as string | undefined,
    memberships: [
      { id: 'm1', role: 'ops', project_id: PROJECT, village_id: null, revoked_at: null as string | null },
    ],
  },
}
vi.mock('@/app/session', () => ({ useSession: () => session }))
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})

const { SurveyAdminListScreen } = await import('@/features/ops/surveys/SurveyAdminListScreen')
const { default: i18n } = await import('@/i18n')

const asAdmin = () => {
  session.data.memberships = [
    { id: 'm2', role: 'admin', project_id: PROJECT, village_id: null, revoked_at: null },
  ]
}

const survey = (over: Record<string, unknown> = {}) => ({
  id: '30000000-0000-4000-8000-000000000001',
  project_id: PROJECT,
  village_id: null,
  title_en: 'Harvest intentions',
  title_sw: 'Nia ya mavuno',
  reward_amount: 5000,
  currency: 'TZS',
  status: 'live',
  closes_at: null,
  published_at: '2026-09-20T08:00:00Z',
  created_at: '2026-09-19T08:00:00Z',
  summary: {
    survey_id: '30000000-0000-4000-8000-000000000001',
    project_id: PROJECT,
    responses: 3,
    issued_count: 3,
    issued_amount: 15000,
    redeemed_count: 1,
    redeemed_amount: 5000,
    outstanding_count: 2,
    outstanding_amount: 10000,
    expired_count: 0,
    void_count: 0,
  },
  ...over,
})

beforeEach(() => {
  useSurveyAdminList.mockReset()
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue({ id: 'new-id' })
  navigate.mockReset()
  createState.isPending = false
  createState.isError = false
  createState.error = null
  session.data.userId = undefined
  session.data.memberships = [
    { id: 'm1', role: 'ops', project_id: PROJECT, village_id: null, revoked_at: null },
  ]
  useSurveyAdminList.mockReturnValue({ isLoading: false, error: null, data: [survey()], refetch: vi.fn() })
})

afterEach(async () => {
  await act(async () => {
    await i18n.changeLanguage('en')
  })
})

describe('SurveyAdminListScreen states', () => {
  test('loading shows a loading state', () => {
    useSurveyAdminList.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<SurveyAdminListScreen />)
    expect(screen.getByTestId('surveys-loading')).toBeInTheDocument()
  })

  test('a failed read is an error with a retry', () => {
    useSurveyAdminList.mockReturnValue({ isLoading: false, error: new Error('read failed'), data: undefined, refetch: vi.fn() })
    render(<SurveyAdminListScreen />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('read failed')
  })

  test('no surveys is an empty state, not an error', () => {
    useSurveyAdminList.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<SurveyAdminListScreen />)
    expect(screen.getByTestId('empty-state')).toHaveTextContent('No surveys yet.')
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('the survey table', () => {
  test('shows the title, status, incentive and the database figures', () => {
    render(<SurveyAdminListScreen />)

    const row = screen.getByTestId('survey-row')
    expect(row).toHaveTextContent('Harvest intentions')
    expect(within(row).getByTestId('status-pill')).toHaveAttribute('data-status', 'live')
    expect(row).toHaveTextContent('TZS 5,000.00')
    expect(within(row).getByTestId('survey-responses')).toHaveTextContent('3')
    expect(within(row).getByTestId('survey-redeemed')).toHaveTextContent('1 · TZS 5,000.00')
    expect(within(row).getByTestId('survey-outstanding')).toHaveTextContent('2 · TZS 10,000.00')
  })

  // A missing summary row is shown as absent, never as an invented zero.
  test('a survey without a summary row shows no figures rather than zeros', () => {
    useSurveyAdminList.mockReturnValue({ isLoading: false, error: null, data: [survey({ summary: null })] })
    render(<SurveyAdminListScreen />)

    expect(screen.getByTestId('survey-responses')).toHaveTextContent('—')
    expect(screen.getByTestId('survey-redeemed')).toHaveTextContent('—')
  })

  test('the title is chosen by language at render', async () => {
    render(<SurveyAdminListScreen />)
    await act(async () => {
      await i18n.changeLanguage('sw')
    })
    expect(screen.getByTestId('survey-row')).toHaveTextContent('Nia ya mavuno')
  })

  test('an untranslated title falls back to English', async () => {
    useSurveyAdminList.mockReturnValue({ isLoading: false, error: null, data: [survey({ title_sw: null })] })
    render(<SurveyAdminListScreen />)
    await act(async () => {
      await i18n.changeLanguage('sw')
    })
    expect(screen.getByTestId('survey-row')).toHaveTextContent('Harvest intentions')
  })

  test('a row opens the survey', async () => {
    render(<SurveyAdminListScreen />)
    await userEvent.click(screen.getByTestId('survey-row'))
    expect(navigate).toHaveBeenCalledWith({
      to: '/ops/surveys/$surveyId',
      params: { surveyId: '30000000-0000-4000-8000-000000000001' },
    })
  })

  // The guided tour opens "a live survey somebody has answered", and a
  // selector can only say that if the row carries it. An unanswered survey has
  // no vouchers to show, so it must not be the one a tour walks into.
  test('a row says its status and whether anyone has answered it', () => {
    useSurveyAdminList.mockReturnValue({
      isLoading: false,
      error: null,
      data: [
        survey(),
        survey({ id: 'a', status: 'live', summary: { ...survey().summary, responses: 0 } }),
        survey({ id: 'b', status: 'draft', summary: null }),
      ],
    })
    render(<SurveyAdminListScreen />)

    const [answered, unanswered, draft] = screen.getAllByTestId('survey-row')
    expect(answered).toHaveAttribute('data-status', 'live')
    expect(answered).toHaveAttribute('data-answered', 'true')
    expect(unanswered).toHaveAttribute('data-answered', 'false')
    expect(draft).toHaveAttribute('data-status', 'draft')
    expect(draft).toHaveAttribute('data-answered', 'false')
  })

  test('the header links to the redemptions log', () => {
    render(<SurveyAdminListScreen />)
    expect(screen.getByTestId('surveys-redemptions-link')).toHaveAttribute('href', '/ops/surveys/redemptions')
  })
})

describe('authoring controls', () => {
  test('ops see no "New survey" control', () => {
    render(<SurveyAdminListScreen />)
    expect(screen.queryByTestId('survey-create-open')).not.toBeInTheDocument()
  })

  test('an admin does', () => {
    asAdmin()
    render(<SurveyAdminListScreen />)
    expect(screen.getByTestId('survey-create-open')).toHaveTextContent('New survey')
    expect(screen.queryByTestId('survey-create-panel')).not.toBeInTheDocument()
  })

  test('a new survey is a draft in the admin project, opened once saved', async () => {
    asAdmin()
    render(<SurveyAdminListScreen />)
    await userEvent.click(screen.getByTestId('survey-create-open'))

    await userEvent.type(screen.getByTestId('survey-new-title'), '  Water use  ')
    await userEvent.type(screen.getByTestId('survey-new-reward'), '5000')
    await userEvent.click(screen.getByTestId('survey-new-submit'))

    expect(mutateAsync).toHaveBeenCalledTimes(1)
    const input = mutateAsync.mock.calls[0][0]
    expect(input).toEqual({
      id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      project_id: PROJECT,
      title_en: 'Water use',
      reward_amount: 5000,
    })
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({ to: '/ops/surveys/$surveyId', params: { surveyId: input.id } }),
    )
  })

  // reward_amount is not null and > 0 in the database, which says so itself.
  test('a blank incentive is sent as null, not pre-checked', async () => {
    asAdmin()
    render(<SurveyAdminListScreen />)
    await userEvent.click(screen.getByTestId('survey-create-open'))
    await userEvent.type(screen.getByTestId('survey-new-title'), 'Water use')

    fireEvent.submit(screen.getByTestId('survey-new-submit').closest('form')!)

    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({ reward_amount: null })
  })

  test('a second submit in the same tick does not write twice', async () => {
    asAdmin()
    render(<SurveyAdminListScreen />)
    await userEvent.click(screen.getByTestId('survey-create-open'))
    await userEvent.type(screen.getByTestId('survey-new-title'), 'Water use')

    const form = screen.getByTestId('survey-new-submit').closest('form')!
    fireEvent.submit(form)
    fireEvent.submit(form)

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
  })

  test('the database refusal is shown', async () => {
    asAdmin()
    createState.isError = true
    createState.error = new Error('only an admin may author surveys')
    render(<SurveyAdminListScreen />)
    await userEvent.click(screen.getByTestId('survey-create-open'))

    expect(screen.getByTestId('error-state')).toHaveTextContent('only an admin may author surveys')
  })

  test('submit is disabled while the write is in flight', async () => {
    asAdmin()
    createState.isPending = true
    render(<SurveyAdminListScreen />)
    await userEvent.click(screen.getByTestId('survey-create-open'))

    expect(screen.getByTestId('survey-new-submit')).toBeDisabled()
  })
})
