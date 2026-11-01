import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const session = { data: { userId: undefined as string | undefined, memberships: [] } }
vi.mock('@/app/session', () => ({ useSession: () => session }))
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})

const { SurveyEditor } = await import('@/features/ops/surveys/SurveyEditor')
await import('@/i18n')

const mutateAsync = vi.fn()
const update = {
  mutateAsync,
  isPending: false,
  isError: false,
  isSuccess: false,
  error: null as Error | null,
  reset: vi.fn(),
}

const survey = {
  id: '30000000-0000-4000-8000-000000000001',
  village_id: null,
  title_en: 'Harvest intentions',
  title_sw: null,
  description_en: null,
  description_sw: null,
  reward: { minor: 500000, decimals: 2, currency: 'TZS', currency_id: 'c-tzs' },
  max_households: null,
  audit_percent: 15,
  responses: 0,
  questions: [],
  closes_at: null,
  status: 'draft' as const,
  published_at: null,
  published_by: null,
  closed_at: null,
  created_by: null,
  created_at: '2026-09-28T08:00:00Z',
  updated_at: '2026-09-28T08:00:00Z',
  summary: null,
}

const renderEditor = () =>
  render(<SurveyEditor survey={survey} update={update as never} />)

beforeEach(() => {
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue(undefined)
  update.isPending = false
  update.isError = false
  update.isSuccess = false
  update.error = null
})

describe('the draft editor', () => {
  test('starts from the stored survey', () => {
    renderEditor()

    expect(screen.getByTestId('survey-title-en')).toHaveValue('Harvest intentions')
    // Integer minor units on the wire; major units in the form.
    expect(screen.getByTestId('survey-reward')).toHaveValue('5000.00')
    expect(screen.getByTestId('survey-audit-rate')).toHaveValue(15)
    expect(screen.getByTestId('survey-title-sw')).toHaveValue('')
  })

  test('Swahili fields are optional and say why', () => {
    renderEditor()
    expect(screen.getAllByText(/native reviewer/i).length).toBeGreaterThan(0)
  })

  test('says what the audit hold means', () => {
    renderEditor()
    expect(screen.getByText(/random share of vouchers/i)).toBeInTheDocument()
  })

  test('saving sends every draft field, the reward as integer minor units', async () => {
    renderEditor()

    await userEvent.clear(screen.getByTestId('survey-title-en'))
    await userEvent.type(screen.getByTestId('survey-title-en'), 'Harvest 2026')
    await userEvent.clear(screen.getByTestId('survey-audit-rate'))
    await userEvent.type(screen.getByTestId('survey-audit-rate'), '20')
    await userEvent.clear(screen.getByTestId('survey-reward'))
    await userEvent.type(screen.getByTestId('survey-reward'), '7,500')
    fireEvent.change(screen.getByTestId('survey-closes-on'), { target: { value: '2026-10-31' } })
    await userEvent.click(screen.getByTestId('survey-save'))

    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toEqual({
      title_en: 'Harvest 2026',
      title_sw: null,
      description_en: null,
      description_sw: null,
      reward_minor: 750000,
      currency_id: 'c-tzs',
      max_households: null,
      audit_percent: 20,
      closes_at: '2026-10-31T23:59:59+03:00',
    })
  })

  // "the closing date must be in the future" and every other rule is the
  // API's. Its sentence is what the admin reads.
  test('the API refusal is shown as written', () => {
    update.isError = true
    update.error = new Error('a live survey cannot be edited')
    renderEditor()

    expect(screen.getByTestId('error-state')).toHaveTextContent('a live survey cannot be edited')
  })

  test('save is disabled while the write is in flight', () => {
    update.isPending = true
    renderEditor()

    expect(screen.getByTestId('survey-save')).toBeDisabled()
    expect(screen.getByTestId('survey-save')).toHaveTextContent('Saving')
  })

  test('a confirmed save says so', () => {
    update.isSuccess = true
    renderEditor()

    expect(screen.getByTestId('survey-saved')).toHaveTextContent('Saved')
  })
})
