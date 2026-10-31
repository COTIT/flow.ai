import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useRequest = vi.fn()
const mutate = vi.fn()
const transitionState = { isPending: false, error: null as Error | null }

const actorName = vi.fn((_id: string | null | undefined): string | undefined => undefined)
vi.mock('@/lib/actorNames', () => ({ useActorName: (id: string | null | undefined) => actorName(id) }))
vi.mock('@/features/farmer/useRequests', () => ({
  useRequest: () => useRequest(),
  useFarmerTransition: () => ({ mutate, ...transitionState }),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ requestId: 'r1' }) }),
}))

const { RequestDetailScreen } = await import('@/features/farmer/RequestDetailScreen')
await import('@/i18n')

beforeEach(() => {
  useRequest.mockReset()
  mutate.mockReset()
  transitionState.isPending = false
  transitionState.error = null
})

const request = (over: Record<string, unknown> = {}) => ({
  id: 'r1',
  source: 'farmer_reported',
  verification: 'unverified',
  confidence: null,
  captured_at: '2026-09-09T12:00:00Z',
  captured_by: null,
  village_id: 'v1',
  status: 'draft',
  quantity: 1,
  hours_per_day: 24,
  days_per_week: 7,
  purpose: 'Chumba cha baridi kwa mboga',
  decision_note: null,
  submitted_at: null,
  equipment_name: 'Cold room 5 kW',
  equipment: { rated_power_kw: 5, indicative_price: 41000000, currency: 'TZS' },
  estimate: {
    est_power_kw: 5,
    est_kwh_per_day: 120,
    est_kwh_per_week: 840,
  },
  ...over,
})

const show = (over: Record<string, unknown> = {}) =>
  useRequest.mockReturnValue({ isLoading: false, error: null, request: request(over) })

/**
 * QA-FINDINGS #5. A draft request rendered its assumptions and stored estimate
 * and offered NO control — no submit, no withdraw. `pue_request_guard` permits
 * `draft -> submitted`, so the transition existed in the database and was
 * unreachable in the UI, leaving the seeded cold-room draft inert.
 *
 * Which controls appear is business-rules §2's role matrix, not a UI choice.
 */
describe('RequestDetailScreen rent or buy', () => {
  test('says whether the farmer asked to rent or to buy', () => {
    show({ acquisition: 'rent' })
    render(<RequestDetailScreen />)
    expect(screen.getByTestId('request-acquisition')).toHaveTextContent(/rent/i)
  })
})

describe('RequestDetailScreen farmer actions', () => {
  test('a draft offers submit and withdraw', () => {
    show({ status: 'draft' })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('request-action-submit')).toBeInTheDocument()
    expect(screen.getByTestId('request-action-withdraw')).toBeInTheDocument()
  })

  test('submitting sends only the action, never a server-stamped field', async () => {
    show({ status: 'draft' })
    render(<RequestDetailScreen />)

    await userEvent.click(screen.getByTestId('request-action-submit'))

    // submitted_at, decided_at and decided_by are the trigger's to set
    // (business-rules §2). The client sends the action and nothing else.
    expect(mutate).toHaveBeenCalledTimes(1)
    expect(mutate).toHaveBeenCalledWith('submit')
  })

  test('a submitted request offers withdraw only', () => {
    show({ status: 'submitted', submitted_at: '2026-09-09T19:20:00Z' })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('request-action-withdraw')).toBeInTheDocument()
    expect(screen.queryByTestId('request-action-submit')).not.toBeInTheDocument()
  })

  // Under review belongs to ops. Offering withdraw here would pull the
  // request out from under a reviewer, and the guard refuses it — an
  // "illegal transition" error reaching a farmer means the UI rendered a
  // control it should not have.
  test('a request under review offers the farmer nothing', () => {
    show({ status: 'under_review' })
    render(<RequestDetailScreen />)

    expect(screen.queryByTestId('request-action-submit')).not.toBeInTheDocument()
    expect(screen.queryByTestId('request-action-withdraw')).not.toBeInTheDocument()
  })

  test.each(['approved', 'rejected', 'withdrawn'])('a %s request offers nothing', (status) => {
    show({ status })
    render(<RequestDetailScreen />)

    expect(screen.queryByTestId('request-action-submit')).not.toBeInTheDocument()
    expect(screen.queryByTestId('request-action-withdraw')).not.toBeInTheDocument()
  })

  test('actions are disabled while a transition is in flight', () => {
    transitionState.isPending = true
    show({ status: 'draft' })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('request-action-submit')).toBeDisabled()
    expect(screen.getByTestId('request-action-withdraw')).toBeDisabled()
  })

  // The guard's messages are written to be read by humans (business-rules §9).
  test("a refusal from the guard is surfaced verbatim", () => {
    transitionState.error = new Error('illegal transition draft -> approved')
    show({ status: 'draft' })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('request-action-error')).toHaveTextContent(
      'illegal transition draft -> approved',
    )
  })
})

describe('RequestDetailScreen content freeze', () => {
  // "A submitted request is not editable. The trigger enforces it; the UI must
  // not offer the control." A draft is not frozen, so it must not claim to be.
  test('a draft is not described as frozen', () => {
    show({ status: 'draft' })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('request-detail')).not.toHaveTextContent(/cannot be changed/i)
  })

  test('a submitted request says its content cannot be changed', () => {
    show({ status: 'submitted' })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('request-detail')).toHaveTextContent(/cannot be changed/i)
  })

  test('no edit control is offered in any state', () => {
    for (const status of ['draft', 'submitted', 'approved']) {
      show({ status })
      const { unmount } = render(<RequestDetailScreen />)
      expect(screen.queryByTestId('request-edit')).not.toBeInTheDocument()
      unmount()
    }
  })
})

describe('RequestDetailScreen states', () => {
  test('loading shows a loading state', () => {
    useRequest.mockReturnValue({ isLoading: true, error: null, request: null })
    render(<RequestDetailScreen />)
    expect(screen.getByTestId('request-detail-loading')).toBeInTheDocument()
  })

  test('a request RLS does not show is an empty state, not an error', () => {
    useRequest.mockReturnValue({ isLoading: false, error: null, request: null })
    render(<RequestDetailScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('RequestDetailScreen provenance', () => {
  const ACTOR = '80000000-0000-4000-8000-000000000005'

  test('the badge names who captured the request, never their raw id', () => {
    actorName.mockImplementation((id) => (id === ACTOR ? 'Neema Mwakalinga' : undefined))
    useRequest.mockReturnValue({ isLoading: false, error: null, request: request({ captured_by: ACTOR }) })
    render(<RequestDetailScreen />)
    const badge = screen.getAllByTestId('provenance-badge')[0]
    expect(badge.getAttribute('title')).toContain('Neema Mwakalinga')
    expect(badge.getAttribute('title')).not.toContain(ACTOR)
  })

  test('an actor the caller may not see is omitted, not shown as an id', () => {
    actorName.mockReturnValue(undefined)
    useRequest.mockReturnValue({ isLoading: false, error: null, request: request({ captured_by: ACTOR }) })
    render(<RequestDetailScreen />)
    expect(screen.getAllByTestId('provenance-badge')[0].getAttribute('title')).not.toContain(ACTOR)
  })
})
