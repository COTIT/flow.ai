import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useVerifyQueue = vi.fn()
const mutate = vi.fn()
const verifyState = { isPending: false, error: null as Error | null }
const scopeState: { data: { villages: Record<string, string>; projects: Record<string, string> } | undefined } = {
  data: { villages: { v1: 'Ilundo' }, projects: {} },
}

vi.mock('@/features/officer/useVerifyQueue', () => ({
  useVerifyQueue: () => useVerifyQueue(),
  useVerifyFromQueue: () => ({ mutate, mutateAsync: mutate, ...verifyState }),
}))
vi.mock('@/app/scope', () => ({
  useScopeNames: () => scopeState,
}))
const sessionState: { data: { userId: string } | undefined } = { data: { userId: 'officer-1' } }
vi.mock('@/app/session', () => ({ useSession: () => sessionState }))
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...props }: { children: React.ReactNode; to: string; [key: string]: unknown }) => (
    <a href={to} {...props}>{children}</a>
  ),
}))

const { VerifyQueueScreen } = await import('@/features/officer/VerifyQueueScreen')
await import('@/i18n')

beforeEach(() => {
  useVerifyQueue.mockReset()
  mutate.mockReset()
  verifyState.isPending = false
  verifyState.error = null
  scopeState.data = { villages: { v1: 'Ilundo' }, projects: {} }
  sessionState.data = { userId: 'officer-1' }
})

const row = (over: Record<string, unknown> = {}) => ({
  table: 'person',
  id: 'p1',
  label: 'Daudi Mbwana',
  village_id: 'v1',
  source: 'farmer_reported',
  verification: 'unverified',
  confidence: 'low',
  captured_at: '2026-09-09T21:30:00Z',
  ...over,
})

/** Spec 5.7 — the verify queue. */
describe('VerifyQueueScreen states', () => {
  test('loading shows a loading state, not an empty message', () => {
    useVerifyQueue.mockReturnValue({ isLoading: true, error: null, data: [] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-queue-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', async () => {
    useVerifyQueue.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: [],
      refetch: vi.fn(),
    })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    await userEvent.setup().click(screen.getByTestId('error-retry'))
  })

  // Nothing outstanding is good news and gets a real message. It must never
  // read as a failure, and a failed READ must never read as an empty queue —
  // that would tell an officer their work is done when it is not.
  test('an empty queue is a real message, not an error', () => {
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
    expect(screen.queryByTestId('verify-queue-count')).not.toBeInTheDocument()
  })
})

describe('VerifyQueueScreen content', () => {
  test('counts the queue and lists each record with its kind and provenance', () => {
    useVerifyQueue.mockReturnValue({
      isLoading: false,
      error: null,
      data: [row(), row({ table: 'farm', id: 'f1', label: 'Shamba la Neema' })],
    })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-queue-count')).toHaveTextContent('2')
    expect(screen.getAllByTestId('verify-queue-row')).toHaveLength(2)
    expect(screen.getByText('Person')).toBeInTheDocument()
    expect(screen.getByText('Farm')).toBeInTheDocument()
    expect(screen.getAllByTestId('provenance-badge')).toHaveLength(2)
  })

  // Spec 5.7 names both statuses, and VerifyButton hides only on 'verified',
  // so a pending record is actionable. A queue holding rows nobody can clear
  // would be worse than one that lets an officer clear them.
  test('offers Verify on pending as well as unverified', () => {
    useVerifyQueue.mockReturnValue({
      isLoading: false,
      error: null,
      data: [row({ verification: 'pending', id: 'p2' })],
    })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-person-p2')).toBeInTheDocument()
  })

  test('renders every supported parent route and plain text when parent is missing', () => {
    useVerifyQueue.mockReturnValue({
      isLoading: false,
      error: null,
      data: [
        row({ table: 'plot', id: 'pl1', farm_id: 'f1' }),
        row({ table: 'harvest_report', id: 'h1', crop_cycle_id: 'c1' }),
        row({ table: 'plot', id: 'pl2', village_id: 'v2' }),
        row({ table: 'harvest_report', id: 'h2', village_id: 'v2' }),
      ],
    })
    render(<VerifyQueueScreen />)
    expect(screen.getAllByTestId('verify-record-link')).toHaveLength(2)
    expect(screen.getAllByTestId('verify-queue-row')).toHaveLength(4)
  })

  test('undefined query data becomes an empty state and null confidence is safe', () => {
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: undefined })
    render(<VerifyQueueScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()

    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [row({ confidence: null })] })
    render(<VerifyQueueScreen />)
    expect(screen.getByTestId('verify-queue-row')).toBeInTheDocument()
  })

  test('falls back to the raw village ID when scope names are unavailable', () => {
    scopeState.data = undefined
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [row()] })
    render(<VerifyQueueScreen />)
    expect(screen.getByTestId('verify-queue-row')).toHaveTextContent('v1')
  })

  test('verifying one record sends only that record', async () => {
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [row()] })
    render(<VerifyQueueScreen />)

    await userEvent.click(screen.getByTestId('verify-person-p1'))
    expect(mutate).not.toHaveBeenCalled()
    await userEvent.click(screen.getByTestId('confirm-dialog-confirm'))

    await waitFor(() => expect(mutate).toHaveBeenCalledTimes(1))
    expect(mutate).toHaveBeenCalledWith({ table: 'person', id: 'p1' })
  })

  test('every verify control is disabled while a verification is in flight', () => {
    verifyState.isPending = true
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [row()] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-person-p1')).toBeDisabled()
  })

  // app_verify's own message, shown as written (business-rules §9).
  test("a refusal from app_verify is surfaced verbatim", () => {
    verifyState.error = new Error('only field staff may verify records')
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [row()] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('error-state')).toHaveTextContent(
      'You do not have permission to do that.',
    )
    // The queue still renders: one failed write does not hide the work.
    expect(screen.getByTestId('verify-queue-row')).toBeInTheDocument()
  })
})

/**
 * Four eyes on households (20260929090002_household_four_eyes): the officer
 * who registered a household may not verify it, because a household is what a
 * survey incentive is paid to. app_verify refuses; the queue says so first,
 * rather than offering a button that can only fail.
 */
describe('VerifyQueueScreen household four eyes', () => {
  const household = (over: Record<string, unknown> = {}) =>
    row({ table: 'household', id: 'hh1', label: 'Kaya ya Neema', captured_by: 'officer-1', ...over })

  test('a household you registered has no Verify button, and says why', () => {
    useVerifyQueue.mockReturnValue({
      isLoading: false,
      error: null,
      data: [household(), row({ id: 'p1', captured_by: undefined })],
    })
    render(<VerifyQueueScreen />)

    expect(screen.queryByTestId('verify-household-hh1')).not.toBeInTheDocument()
    expect(screen.getByTestId('verify-needs-second-staff-hh1')).toBeInTheDocument()
    // The rule is about households only: your own person record stays verifiable.
    expect(screen.getByTestId('verify-person-p1')).toBeInTheDocument()
    // The row still counts: it is outstanding, for someone else.
    expect(screen.getByTestId('verify-queue-count')).toHaveTextContent('2')
  })

  test("another officer's household can be verified", () => {
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [household({ captured_by: 'officer-2' })] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-household-hh1')).toBeInTheDocument()
    expect(screen.queryByTestId('verify-needs-second-staff-hh1')).not.toBeInTheDocument()
  })

  test('a household with no recorded registrar can be verified', () => {
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [household({ captured_by: null })] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-household-hh1')).toBeInTheDocument()
  })

  // Convenience, not security: with no session read yet, the button is shown
  // and the database is still the one that refuses.
  test('before the session is known, the button is offered and the database decides', () => {
    sessionState.data = undefined
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [household()] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('verify-household-hh1')).toBeInTheDocument()
  })

  test("the database's refusal is still shown verbatim if it comes", () => {
    verifyState.error = new Error('a household must be verified by someone other than the officer who registered it')
    useVerifyQueue.mockReturnValue({ isLoading: false, error: null, data: [household({ captured_by: 'officer-2' })] })
    render(<VerifyQueueScreen />)

    expect(screen.getByTestId('error-state')).toHaveTextContent(
      'A household must be verified by someone other than the officer who registered it.',
    )
  })
})
