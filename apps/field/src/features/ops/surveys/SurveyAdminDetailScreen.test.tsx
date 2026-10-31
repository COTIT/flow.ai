import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

const useSurveyAdminDetail = vi.fn()
const statusMutateAsync = vi.fn()
const statusState = { isPending: false, isError: false, error: null as Error | null }
const idle = { isPending: false, isError: false, isSuccess: false, error: null, reset: vi.fn() }

vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useSurveyAdminDetail: (id: string) => useSurveyAdminDetail(id),
  useUpdateSurvey: () => ({ ...idle, mutateAsync: vi.fn() }),
  useSetSurveyStatus: () => ({ ...idle, mutateAsync: statusMutateAsync, ...statusState }),
  useSaveQuestion: () => ({ ...idle, mutateAsync: vi.fn() }),
  useRemoveQuestion: () => ({ ...idle, mutate: vi.fn() }),
  useSwapQuestions: () => ({ ...idle, mutate: vi.fn() }),
  useSurveyTally: () => ({ isLoading: false, error: null, data: { choices: [], numbers: [] } }),
  useSurveyVouchers: () => ({ isLoading: false, error: null, data: [] }),
  useVoucherTimeline: () => ({ isLoading: false, error: null, data: undefined }),
  useVoidVoucher: () => ({ ...idle, mutateAsync: vi.fn() }),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ surveyId: '30000000-0000-4000-8000-000000000001' }) }),
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))
const names: Record<string, string> = { u1: 'Amina Admin', u2: 'Juma Admin' }
vi.mock('@/lib/actorNames', () => ({
  useActorName: (id: string | null | undefined) => (id ? names[id] : undefined),
}))

const PROJECT = '20000000-0000-4000-8000-000000000001'
const session = {
  data: {
    userId: undefined as string | undefined,
    memberships: [] as Array<Record<string, unknown>>,
  },
}
vi.mock('@/app/session', () => ({ useSession: () => session }))
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})

const { SurveyAdminDetailScreen } = await import('@/features/ops/surveys/SurveyAdminDetailScreen')
const { default: i18n } = await import('@/i18n')

const as = (role: 'admin' | 'ops') => {
  session.data.memberships = [{ id: 'm1', role, project_id: PROJECT, village_id: null, revoked_at: null }]
}

const survey = (over: Record<string, unknown> = {}) => ({
  id: '30000000-0000-4000-8000-000000000001',
  project_id: PROJECT,
  village_id: null,
  title_en: 'Harvest intentions',
  title_sw: 'Nia ya mavuno',
  description_en: null,
  description_sw: null,
  reward_amount: 5000,
  currency: 'TZS',
  max_households: null,
  audit_rate: 0.15,
  closes_at: null,
  status: 'draft',
  published_at: null,
  published_by: null,
  closed_at: null,
  created_by: 'u1',
  created_at: '2026-09-19T08:00:00Z',
  updated_at: '2026-09-19T08:00:00Z',
  ...over,
})

const live = () =>
  survey({ status: 'live', published_at: '2026-09-20T08:00:00Z', published_by: 'u2', closes_at: '2026-10-31T20:59:59+00:00' })

const question = {
  id: 'q1',
  survey_id: '30000000-0000-4000-8000-000000000001',
  position: 1,
  kind: 'yes_no',
  prompt_en: 'Do you irrigate?',
  prompt_sw: null,
  required: true,
  options: [],
  deleted_at: null,
  created_at: '2026-09-19T08:00:00Z',
  updated_at: '2026-09-19T08:00:00Z',
}

const detail = (s = survey()) => ({
  isLoading: false,
  error: null,
  refetch: vi.fn(),
  data: { survey: s, questions: [question], summary: null },
})

beforeEach(() => {
  useSurveyAdminDetail.mockReset()
  statusMutateAsync.mockReset()
  statusMutateAsync.mockResolvedValue(undefined)
  Object.assign(statusState, { isPending: false, isError: false, error: null })
  as('ops')
  useSurveyAdminDetail.mockReturnValue(detail())
})

afterEach(async () => {
  await act(async () => {
    await i18n.changeLanguage('en')
  })
})

describe('SurveyAdminDetailScreen states', () => {
  test('reads the survey named by the route', () => {
    render(<SurveyAdminDetailScreen />)
    expect(useSurveyAdminDetail).toHaveBeenCalledWith('30000000-0000-4000-8000-000000000001')
  })

  test('loading', () => {
    useSurveyAdminDetail.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<SurveyAdminDetailScreen />)
    expect(screen.getByTestId('survey-detail-loading')).toBeInTheDocument()
  })

  test('a failed read is an error', () => {
    useSurveyAdminDetail.mockReturnValue({ isLoading: false, error: new Error('read failed'), data: undefined, refetch: vi.fn() })
    render(<SurveyAdminDetailScreen />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('read failed')
  })

  test('zero rows is not found, not an error', () => {
    useSurveyAdminDetail.mockReturnValue({ isLoading: false, error: null, data: null })
    render(<SurveyAdminDetailScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('the header', () => {
  test('names the survey, its status, incentive and audit rate', () => {
    render(<SurveyAdminDetailScreen />)

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Harvest intentions')
    expect(screen.getByTestId('page-header')).toContainElement(screen.getAllByTestId('status-pill')[0])
    expect(screen.getByTestId('survey-fact-reward')).toHaveTextContent('TZS 5,000.00')
    expect(screen.getByTestId('survey-fact-audit')).toHaveTextContent('15.0%')
    expect(screen.getByTestId('survey-created-by')).toHaveTextContent('Created by Amina Admin')
  })

  test('a published survey says who published it and when', () => {
    useSurveyAdminDetail.mockReturnValue(detail(live()))
    render(<SurveyAdminDetailScreen />)

    expect(screen.getByTestId('survey-published-by')).toHaveTextContent('Published 20 Sep 2026, 11:00 by Juma Admin')
    expect(screen.getByTestId('survey-fact-closes')).toHaveTextContent('31 Oct 2026')
  })

  test('the title is chosen by language at render', async () => {
    render(<SurveyAdminDetailScreen />)
    await act(async () => {
      await i18n.changeLanguage('sw')
    })
    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Nia ya mavuno')
  })
})

describe('a draft', () => {
  test('an admin gets the editor, the questions and Publish', () => {
    as('admin')
    render(<SurveyAdminDetailScreen />)

    expect(screen.getByTestId('survey-editor')).toBeInTheDocument()
    expect(screen.getByTestId('questions-editor')).toBeInTheDocument()
    expect(screen.getByTestId('survey-publish')).toHaveTextContent('Publish')
    expect(screen.queryByTestId('survey-results')).not.toBeInTheDocument()
  })

  test('ops read it, with no authoring controls', () => {
    render(<SurveyAdminDetailScreen />)

    expect(screen.queryByTestId('survey-editor')).not.toBeInTheDocument()
    expect(screen.queryByTestId('questions-editor')).not.toBeInTheDocument()
    expect(screen.queryByTestId('survey-publish')).not.toBeInTheDocument()
    expect(screen.getByText('Only an admin can author surveys.')).toBeInTheDocument()
    expect(screen.getByTestId('question-list')).toHaveTextContent('Do you irrigate?')
  })

  test('publishing asks first, then sets it live', async () => {
    as('admin')
    render(<SurveyAdminDetailScreen />)

    await userEvent.click(screen.getByTestId('survey-publish'))
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('Publish this survey?')
    expect(statusMutateAsync).not.toHaveBeenCalled()

    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))
    expect(statusMutateAsync).toHaveBeenCalledWith('live')
    await waitFor(() => expect(screen.queryByTestId('confirm-dialog')).not.toBeInTheDocument())
  })

  test('backing out publishes nothing', async () => {
    as('admin')
    render(<SurveyAdminDetailScreen />)

    await userEvent.click(screen.getByTestId('survey-publish'))
    await userEvent.click(screen.getByTestId('confirm-dialog-cancel'))

    expect(statusMutateAsync).not.toHaveBeenCalled()
  })

  // Every precondition of going live is survey_guard's.
  test('the guard refusal is shown as written', () => {
    as('admin')
    statusState.isError = true
    statusState.error = new Error('a survey needs at least one question before it goes live')
    render(<SurveyAdminDetailScreen />)

    expect(screen.getByTestId('survey-status-error')).toHaveTextContent(
      'a survey needs at least one question before it goes live',
    )
  })
})

describe('a live survey', () => {
  test('shows results and vouchers, and no editor even for an admin', () => {
    as('admin')
    useSurveyAdminDetail.mockReturnValue(detail(live()))
    render(<SurveyAdminDetailScreen />)

    expect(screen.getByTestId('survey-results')).toBeInTheDocument()
    expect(screen.getByTestId('survey-vouchers')).toBeInTheDocument()
    expect(screen.queryByTestId('survey-editor')).not.toBeInTheDocument()
    expect(screen.queryByTestId('survey-publish')).not.toBeInTheDocument()
  })

  test('an admin can close it, after confirming', async () => {
    as('admin')
    useSurveyAdminDetail.mockReturnValue(detail(live()))
    render(<SurveyAdminDetailScreen />)

    await userEvent.click(screen.getByTestId('survey-close'))
    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('Close this survey?')
    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(statusMutateAsync).toHaveBeenCalledWith('closed')
  })

  test('ops see the results but cannot close it', () => {
    useSurveyAdminDetail.mockReturnValue(detail(live()))
    render(<SurveyAdminDetailScreen />)

    expect(screen.getByTestId('survey-results')).toBeInTheDocument()
    expect(screen.queryByTestId('survey-close')).not.toBeInTheDocument()
  })

  test('a closed survey offers no status change', () => {
    as('admin')
    useSurveyAdminDetail.mockReturnValue(detail(survey({ status: 'closed', published_by: 'u2', published_at: '2026-09-20T08:00:00Z' })))
    render(<SurveyAdminDetailScreen />)

    expect(screen.queryByTestId('survey-close')).not.toBeInTheDocument()
    expect(screen.queryByTestId('survey-publish')).not.toBeInTheDocument()
    expect(screen.getByTestId('survey-results')).toBeInTheDocument()
  })
})
