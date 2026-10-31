import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSurveyVouchers = vi.fn()
const useVoucherTimeline = vi.fn()
const voidMutateAsync = vi.fn()
const voidState = { isPending: false, isError: false, error: null as Error | null }

vi.mock('@/features/ops/surveys/useSurveyAdmin', () => ({
  useSurveyVouchers: (id: string) => useSurveyVouchers(id),
  useVoucherTimeline: (id: string | null) => useVoucherTimeline(id),
  useVoidVoucher: () => ({ mutateAsync: voidMutateAsync, reset: vi.fn(), ...voidState }),
}))

const { SurveyVouchers } = await import('@/features/ops/surveys/SurveyVouchers')
await import('@/i18n')

const SURVEY = '30000000-0000-4000-8000-000000000001'

const voucher = (over: Record<string, unknown> = {}) => ({
  voucher_id: '40000000-0000-4000-8000-000000000001',
  household_label: 'Mwakalinga household',
  respondent_name: 'Neema Mwakalinga',
  submitted_at: '2026-09-20T07:30:00Z',
  status: 'issued',
  expired: false,
  amount: 5000,
  currency: 'TZS',
  audit_required: false,
  issued_at: '2026-09-20T07:30:00Z',
  expires_at: '2026-10-20T07:30:00Z',
  redeemed_at: null,
  redeemed_by_name: null,
  id_type_seen: null,
  void_reason: null,
  ...over,
})

const redeemed = voucher({
  voucher_id: '40000000-0000-4000-8000-000000000002',
  household_label: 'Mgeni household',
  respondent_name: 'Baraka Mgeni',
  status: 'redeemed',
  redeemed_at: '2026-09-22T09:00:00Z',
  redeemed_by_name: 'Asha Officer',
  id_type_seen: 'nida',
  audit_required: true,
})

const timeline = [
  { occurred_at: '2026-09-20T07:30:00Z', kind: 'answered', actor_name: 'Neema Mwakalinga', actor_role: 'farmer', detail: {} },
  { occurred_at: '2026-09-20T07:30:01Z', kind: 'issued', actor_name: 'Neema Mwakalinga', actor_role: 'farmer', detail: {} },
]

beforeEach(() => {
  useSurveyVouchers.mockReset()
  useVoucherTimeline.mockReset()
  voidMutateAsync.mockReset()
  voidMutateAsync.mockResolvedValue(undefined)
  Object.assign(voidState, { isPending: false, isError: false, error: null })
  useSurveyVouchers.mockReturnValue({ isLoading: false, error: null, data: [voucher(), redeemed], refetch: vi.fn() })
  useVoucherTimeline.mockImplementation((id: string | null) =>
    id ? { isLoading: false, error: null, data: timeline, refetch: vi.fn() } : { isLoading: false, error: null, data: undefined },
  )
})

const rows = () => screen.getAllByTestId('voucher-row')

describe('the voucher table', () => {
  test('reads app_survey_vouchers for this survey', () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    expect(useSurveyVouchers).toHaveBeenCalledWith(SURVEY)
  })

  test('shows who answered, when, the status and the incentive', () => {
    render(<SurveyVouchers surveyId={SURVEY} />)

    const [first, second] = rows()
    expect(first).toHaveTextContent('Mwakalinga household')
    expect(first).toHaveTextContent('Neema Mwakalinga')
    expect(first).toHaveTextContent('20 Sep 2026, 10:30')
    expect(within(first).getByTestId('status-pill')).toHaveAttribute('data-status', 'issued')
    expect(first).toHaveTextContent('TZS 5,000.00')
    expect(second).toHaveTextContent('Asha Officer')
    expect(within(second).getByTestId('status-pill')).toHaveAttribute('data-status', 'redeemed')
  })

  test('marks a voucher held for audit', () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    const [first, second] = rows()
    expect(within(first).queryByTestId('voucher-held')).not.toBeInTheDocument()
    expect(within(second).getByTestId('voucher-held')).toHaveTextContent('Held')
  })

  // `expired` is derived by the RPC from expires_at; the stored status stays
  // 'issued'. The pill says expired.
  test('an expired voucher reads as expired', () => {
    useSurveyVouchers.mockReturnValue({ isLoading: false, error: null, data: [voucher({ expired: true })] })
    render(<SurveyVouchers surveyId={SURVEY} />)
    expect(within(rows()[0]).getByTestId('status-pill')).toHaveAttribute('data-status', 'expired')
  })

  test('no vouchers is an empty state', () => {
    useSurveyVouchers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<SurveyVouchers surveyId={SURVEY} />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })

  test('a failed read is an error', () => {
    useSurveyVouchers.mockReturnValue({ isLoading: false, error: new Error('only ops'), data: undefined, refetch: vi.fn() })
    render(<SurveyVouchers surveyId={SURVEY} />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('only ops')
  })

  test('loading is a loading state', () => {
    useSurveyVouchers.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<SurveyVouchers surveyId={SURVEY} />)
    expect(screen.getByTestId('vouchers-loading')).toBeInTheDocument()
  })
})

describe('one voucher', () => {
  test('no timeline is read until a voucher is chosen', () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    expect(useVoucherTimeline).toHaveBeenLastCalledWith(null)
    expect(screen.queryByTestId('audit-timeline')).not.toBeInTheDocument()
    expect(screen.getByTestId('voucher-select-prompt')).toBeInTheDocument()
  })

  test('choosing one shows its full audit trail', async () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])

    expect(useVoucherTimeline).toHaveBeenLastCalledWith('40000000-0000-4000-8000-000000000001')
    const panel = screen.getByTestId('voucher-panel')
    expect(panel).toHaveTextContent('Mwakalinga household')
    expect(within(panel).getByTestId('audit-timeline')).toBeInTheDocument()
    expect(within(panel).getByTestId('audit-event-issued')).toBeInTheDocument()
  })

  test('a failed timeline read is an error', async () => {
    useVoucherTimeline.mockImplementation((id: string | null) =>
      id ? { isLoading: false, error: new Error('timeline failed'), data: undefined, refetch: vi.fn() } : { isLoading: false, error: null },
    )
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])

    expect(within(screen.getByTestId('voucher-panel')).getByTestId('error-state')).toHaveTextContent('timeline failed')
  })

  test('a redeemed voucher cannot be cancelled', async () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[1])

    expect(screen.queryByTestId('voucher-void')).not.toBeInTheDocument()
  })
})

describe('cancelling a voucher', () => {
  test('asks first, then voids it with the reason given', async () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])

    await userEvent.type(screen.getByTestId('voucher-void-reason'), 'Household answered twice')
    await userEvent.click(screen.getByTestId('voucher-void'))

    expect(screen.getByTestId('confirm-dialog')).toHaveTextContent('Cancel this voucher?')
    expect(voidMutateAsync).not.toHaveBeenCalled()

    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(voidMutateAsync).toHaveBeenCalledWith({
      voucherId: '40000000-0000-4000-8000-000000000001',
      reason: 'Household answered twice',
    })
    await waitFor(() => expect(screen.queryByTestId('confirm-dialog')).not.toBeInTheDocument())
  })

  test('backing out of the dialog voids nothing', async () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])
    await userEvent.click(screen.getByTestId('voucher-void'))
    await userEvent.click(screen.getByTestId('confirm-dialog-cancel'))

    expect(voidMutateAsync).not.toHaveBeenCalled()
    expect(screen.queryByTestId('confirm-dialog')).not.toBeInTheDocument()
  })

  // "give a reason for voiding this voucher" is the RPC's rule and sentence.
  test('a blank reason is sent, not pre-checked', async () => {
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])
    await userEvent.click(screen.getByTestId('voucher-void'))
    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    expect(voidMutateAsync).toHaveBeenCalledWith({ voucherId: expect.any(String), reason: '' })
  })

  test('the refusal is shown as the database wrote it', async () => {
    voidState.isError = true
    voidState.error = new Error('give a reason for voiding this voucher')
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])

    expect(within(screen.getByTestId('voucher-panel')).getByTestId('error-state')).toHaveTextContent(
      'give a reason for voiding this voucher',
    )
  })

  test('an expired voucher can still be cancelled: it was never collected', async () => {
    useSurveyVouchers.mockReturnValue({ isLoading: false, error: null, data: [voucher({ expired: true })] })
    render(<SurveyVouchers surveyId={SURVEY} />)
    await userEvent.click(rows()[0])

    expect(screen.getByTestId('voucher-void')).toBeInTheDocument()
  })
})
