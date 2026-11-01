import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useSession = vi.fn()
const useEquipmentItem = vi.fn()
const mutateAsync = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ equipmentId: 'eq1' }) }),
  Link: ({ children }: { children: React.ReactNode }) => <a href="#x">{children}</a>,
}))
vi.mock('@/app/session', () => ({ useSession: () => useSession() }))
// In-memory in place of IndexedDB, so a stored draft can be inspected.
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})
vi.mock('@/features/farmer/useEquipment', () => ({
  useEquipmentItem: () => useEquipmentItem(),
}))
const useSubmitRequestState = {
  isPending: false,
  isError: false,
  isSuccess: false,
}
vi.mock('@/features/farmer/useRequests', () => ({
  // No `mutate`: its per-call callbacks are skipped after unmount, so the
  // screen must chain the draft's finish on mutateAsync instead.
  useSubmitRequest: () => ({ mutateAsync, reset: vi.fn(), ...useSubmitRequestState }),
}))

const { EquipmentDetailScreen } = await import('@/features/farmer/EquipmentDetailScreen')
const { indexedDbDraftStore } = await import('@/lib/drafts')
await import('@/i18n')

const VILLAGE = '30000000-0000-4000-8000-000000000001'

beforeEach(() => {
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue({ id: 'r1' })
  useSubmitRequestState.isPending = false
  useSubmitRequestState.isError = false
  useSubmitRequestState.isSuccess = false
  useSession.mockReturnValue({
    isLoading: false,
    error: null,
    data: {
      appUser: { id: 'u1', person_id: 'p1' },
      memberships: [
        { id: 'm1', role: 'farmer', project_id: 'pr1', village_id: VILLAGE, revoked_at: null },
      ],
    },
  })
  useEquipmentItem.mockReturnValue({
    isLoading: false,
    error: null,
    item: {
      id: 'eq1',
      name: 'Mill',
      category_name: 'Processing',
      rated_power_kw: 15,
      indicative_price: 1000,
      currency: 'TZS',
      typical_hours_per_day: 6,
      typical_days_per_week: 5,
      can_rent: false,
      can_buy: true,
      indicative_rent_per_day: null,
    },
  })
})

const set = (testId: string, value: string) =>
  fireEvent.change(screen.getByTestId(testId), { target: { value } })
const submit = () => fireEvent.click(screen.getByTestId('request-submit'))

describe('the request form as it arrives', () => {
  // Prefilled from the equipment's typicals, so the common case is one click.
  test('is prefilled from the equipment typicals', () => {
    render(<EquipmentDetailScreen />)

    expect(screen.getByTestId('request-quantity')).toHaveValue('1')
    expect(screen.getByTestId('request-hours')).toHaveValue('6')
    expect(screen.getByTestId('request-days')).toHaveValue('5')
  })

  test('shows the estimate for those values', () => {
    render(<EquipmentDetailScreen />)
    expect(screen.getByTestId('estimate-panel')).toBeInTheDocument()
  })

  test('submits the numbers that were shown', async () => {
    render(<EquipmentDetailScreen />)
    submit()

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({
      villageId: VILLAGE,
      personId: 'p1',
      equipmentId: 'eq1',
      quantity: 1,
      hoursPerDay: 6,
      daysPerWeek: 5,
    })
  })

  test('a purpose of spaces is not stored as a space', async () => {
    render(<EquipmentDetailScreen />)
    set('request-purpose', '   ')
    submit()

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0].purpose).toBe('')
  })
})

/**
 * QA #9, row by row. The farmer was shown a computed, confident, impossible
 * figure and only learnt it was impossible after submitting.
 */
describe('impossible assumptions are refused before the round trip', () => {
  test('99 hours in a day is refused inline', async () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '99')
    submit()

    const error = await screen.findByTestId('request-hours-error')
    expect(error).toHaveTextContent(/0 to 24/i)
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  // The column allows 0. A request to run a mill for zero hours asks for
  // nothing, so the form refuses to send it.
  test('zero hours is refused, and says why', async () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '0')
    submit()

    const error = await screen.findByTestId('request-hours-error')
    expect(error).toHaveTextContent(/more than zero/i)
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('zero machines is refused — the column checks quantity > 0', async () => {
    render(<EquipmentDetailScreen />)
    set('request-quantity', '0')
    submit()

    await waitFor(() => expect(screen.getByTestId('request-quantity-error')).toBeInTheDocument())
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('eight days in a week is refused', async () => {
    render(<EquipmentDetailScreen />)
    set('request-days', '8')
    submit()

    const error = await screen.findByTestId('request-days-error')
    expect(error).toHaveTextContent(/0 to 7/i)
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('half a machine is refused', async () => {
    render(<EquipmentDetailScreen />)
    set('request-quantity', '1.5')
    submit()

    const error = await screen.findByTestId('request-quantity-error')
    expect(error).toHaveTextContent(/whole number/i)
  })

  test('text in a number field says so', async () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', 'abc')
    submit()

    expect(await screen.findByTestId('request-hours-error')).toHaveTextContent(/enter a number/i)
  })

  // A constraint identifier is not user copy. That is the point of catching it
  // here rather than reading `pue_request_hours_per_day_check` back (QA #4).
  test('and never as a constraint name', async () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '99')
    submit()

    const error = await screen.findByTestId('request-hours-error')
    expect(error).not.toHaveTextContent(/pue_request|check|equipment\./)
  })
})

/**
 * The finding's real complaint: not just that 99 was accepted, but that the
 * screen computed 1,485 kWh/day from it and presented that as an answer.
 */
describe('the estimate does not compute from impossible inputs', () => {
  test('an out-of-range hour count replaces the figures with a reason', () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '99')

    expect(screen.queryByTestId('estimate-panel')).not.toBeInTheDocument()
    expect(screen.getByTestId('estimate-blocked')).toBeInTheDocument()
  })

  test('so does a blank one', () => {
    render(<EquipmentDetailScreen />)
    set('request-days', '')

    expect(screen.queryByTestId('estimate-panel')).not.toBeInTheDocument()
  })

  test('and the figures come back when the inputs make sense again', () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '99')
    expect(screen.queryByTestId('estimate-panel')).not.toBeInTheDocument()

    set('request-hours', '8')
    expect(screen.getByTestId('estimate-panel')).toBeInTheDocument()
    expect(screen.queryByTestId('estimate-blocked')).not.toBeInTheDocument()
  })

  test('an unanswered field is asked for, not reported as an error', () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '')

    const blocked = screen.getByTestId('estimate-blocked')
    expect(blocked).toHaveTextContent(/fill in/i)
    expect(blocked).not.toHaveTextContent(/0\.000/)
  })

  /**
   * QA #32. The copy assumed emptiness: at 99 hours it read "Fill in how many,
   * hours per day and days per week", when the fields ARE filled and one value
   * is simply impossible. It told the farmer to do something they had already
   * done.
   */
  test('an answered but impossible field says that, not "fill it in"', () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '99')

    const blocked = screen.getByTestId('estimate-blocked')
    expect(blocked).not.toHaveTextContent(/fill in/i)
    expect(blocked).toHaveTextContent(/cannot be estimated|not possible/i)
  })

  test('and the same for a number that is not a number', () => {
    render(<EquipmentDetailScreen />)
    set('request-quantity', 'abc')

    expect(screen.getByTestId('estimate-blocked')).not.toHaveTextContent(/fill in/i)
  })

  // A blank AND an impossible value together: the blank is the thing the
  // farmer can act on first.
  test('a blank alongside an impossible value is still asked for', () => {
    render(<EquipmentDetailScreen />)
    set('request-hours', '99')
    set('request-days', '')

    expect(screen.getByTestId('estimate-blocked')).toHaveTextContent(/fill in/i)
  })
})

/** QA #23, on the farmer's side of the same pattern. */
describe('the request form in flight', () => {
  test('a second submit in the same tick does not send a second request', async () => {
    render(<EquipmentDetailScreen />)

    submit()
    submit()

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync).toHaveBeenCalledTimes(1)
  })

  test('the control is disabled while the write is in flight', () => {
    useSubmitRequestState.isPending = true
    render(<EquipmentDetailScreen />)

    expect(screen.getByTestId('request-submit')).toBeDisabled()
  })
})

describe('the draft after a confirmed save', () => {
  // The farmer can navigate away while the request is in flight. The save
  // still lands, so the stored draft must still be cleared.
  test('is cleared even when the screen unmounted before the save resolved', async () => {
    const key = 'form:u1:eq1:equipment-request'
    useSession.mockReturnValue({
      isLoading: false,
      error: null,
      data: {
        userId: 'u1',
        appUser: { id: 'u1', person_id: 'p1' },
        memberships: [
          { id: 'm1', role: 'farmer', project_id: 'pr1', village_id: VILLAGE, revoked_at: null },
        ],
      },
    })
    let resolve!: (value: { id: string }) => void
    mutateAsync.mockReturnValue(new Promise((r) => { resolve = r }))

    const { unmount } = render(<EquipmentDetailScreen />)
    await waitFor(() => expect(screen.getByTestId('request-submit')).toBeEnabled())
    set('request-purpose', 'Milling for the co-op')
    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toBeDefined())

    submit()
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    unmount()
    await act(async () => resolve({ id: 'r1' }))

    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toBeUndefined())
  })
})

/**
 * A machine may be offered to rent, to buy, or both. The farmer says which;
 * whether it is offered that way is the database's call.
 */
describe('rent or buy', () => {
  const offered = (over: Record<string, unknown>) =>
    useEquipmentItem.mockReturnValue({
      isLoading: false,
      error: null,
      item: {
        id: 'eq1', name: 'Mill', category_name: 'Processing', rated_power_kw: 15,
        indicative_price: 1000, indicative_rent_per_day: 50, currency: 'TZS',
        typical_hours_per_day: 6, typical_days_per_week: 5, ...over,
      },
    })

  test('a machine offered only for sale is requested to buy, with nothing to choose', async () => {
    offered({ can_rent: false, can_buy: true })
    render(<EquipmentDetailScreen />)

    expect(screen.getByTestId('equipment-acquisition')).toHaveTextContent(/buy/i)
    expect(screen.queryByTestId('equipment-acquisition-rent')).not.toBeInTheDocument()
    submit()

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({ acquisition: 'buy' })
  })

  test('offered both ways, the farmer has to choose', async () => {
    offered({ can_rent: true, can_buy: true })
    render(<EquipmentDetailScreen />)
    submit()

    expect(await screen.findByTestId('request-acquisition-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('and renting sends rent, with the indicative rent per day shown', async () => {
    offered({ can_rent: true, can_buy: true })
    render(<EquipmentDetailScreen />)

    expect(screen.getByTestId('equipment-rent')).toHaveTextContent('TZS 50.00')
    expect(screen.getByTestId('equipment-rent')).toHaveTextContent(/indicative/i)
    fireEvent.click(screen.getByTestId('equipment-acquisition-rent'))
    submit()

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({ acquisition: 'rent' })
  })
})
