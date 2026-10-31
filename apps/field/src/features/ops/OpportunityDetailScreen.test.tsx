import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useOpportunity = vi.fn()
const useAvailableHarvest = vi.fn()
const useAttachSupply = vi.fn()
const useOpportunityStatus = vi.fn()

vi.mock('@/features/ops/useOpportunity', () => ({
  useOpportunity: () => useOpportunity(),
  useAvailableHarvest: () => useAvailableHarvest(),
  useAttachSupply: () => useAttachSupply(),
  useOpportunityStatus: () => useOpportunityStatus(),
}))
// No signed-in owner: the draft is in-memory only, so the form is ready at once.
// One test signs in, to see the stored draft.
const session = { data: undefined as { userId: string } | undefined }
vi.mock('@/app/session', () => ({ useSession: () => session }))
// In-memory in place of IndexedDB, so a stored draft can be inspected.
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})
// Names resolve through app_actor_names; here, a fixed map.
const actorNames: Record<string, string> = { 'u-ops': 'Fixture Ops' }
vi.mock('@/lib/actorNames', () => ({
  useActorName: (id: string | null | undefined) => (id ? actorNames[id] : undefined),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ opportunityId: 'o1' }) }),
  Link: ({ children }: { children: React.ReactNode }) => <a href="#x">{children}</a>,
}))

const { OpportunityDetailScreen } = await import('@/features/ops/OpportunityDetailScreen')
const { indexedDbDraftStore } = await import('@/lib/drafts')
await import('@/i18n')

const statusMutate = vi.fn()
const attachMutateAsync = vi.fn()

beforeEach(() => {
  useOpportunity.mockReset()
  useAvailableHarvest.mockReset()
  useAttachSupply.mockReset()
  useOpportunityStatus.mockReset()
  statusMutate.mockReset()
  attachMutateAsync.mockReset()
  attachMutateAsync.mockResolvedValue(undefined)
  session.data = undefined

  useAvailableHarvest.mockReturnValue({ isLoading: false, data: [] })
  useAttachSupply.mockReturnValue({
    // No `mutate`: its per-call callbacks are skipped after unmount.
    mutateAsync: attachMutateAsync,
    isPending: false,
    isError: false,
    error: null,
    reset: vi.fn(),
  })
  useOpportunityStatus.mockReturnValue({
    mutate: statusMutate,
    isPending: false,
    isError: false,
    error: null,
    reset: vi.fn(),
  })
})

const opportunity = (over: Record<string, unknown> = {}) => ({
  id: 'o1',
  village_id: 'v1',
  crop_id: 'c1',
  status: 'proposed',
  buyer_name: 'Iringa Grain Traders',
  crop_name: 'Maize',
  village_name: 'Ilundo',
  demand_quantity_kg: 9000,
  offered_quantity_kg: 6400,
  supply: [
    {
      harvest_report_id: 'h1',
      crop_cycle_id: 'cy1',
      contributed_kg: 4100,
      farmer: 'Neema Mwaipopo',
      person_id: 'p1',
      plot_label: 'Kipande cha juu',
      crop_name: 'Maize',
      captured_by: 'u-ops',
      captured_at: '2026-10-01T09:30:00Z',
    },
  ],
  ...over,
})

const loaded = (over: Record<string, unknown> = {}) =>
  useOpportunity.mockReturnValue({
    isLoading: false,
    error: null,
    opportunity: opportunity(over),
    refetch: vi.fn(),
  })

async function chooseHarvest(label: RegExp = /4,100/) {
  await userEvent.click(screen.getByTestId('attach-harvest'))
  await userEvent.click(await screen.findByRole('option', { name: label }))
}

describe('OpportunityDetailScreen states', () => {
  test('loading shows a loading state', () => {
    useOpportunity.mockReturnValue({ isLoading: true, error: null, opportunity: null })
    render(<OpportunityDetailScreen />)
    expect(screen.getByTestId('opportunity-loading')).toBeInTheDocument()
  })

  test('an error is an error', () => {
    useOpportunity.mockReturnValue({
      isLoading: false,
      error: new Error('nope'),
      opportunity: null,
      refetch: vi.fn(),
    })
    render(<OpportunityDetailScreen />)
    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })

  // Zero rows is an answer: RLS says this opportunity is not visible.
  test('no opportunity is an empty state, not an error', () => {
    useOpportunity.mockReturnValue({ isLoading: false, error: null, opportunity: null })
    render(<OpportunityDetailScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByTestId('error-state')).not.toBeInTheDocument()
  })
})

/**
 * QA #12, third bullet. The header read "Quantity 9,000.00 kg" above "Offered
 * 6,400.00 kg". The 9,000 is the BUYER'S DEMAND. On a screen whose entire job
 * is keeping demand and supply distinct, "Quantity" names neither.
 */
describe('the two quantities are named', () => {
  test('the demand figure says whose it is', () => {
    loaded()
    render(<OpportunityDetailScreen />)

    const row = screen.getByTestId('demand-quantity-row')
    expect(row).toHaveTextContent('9,000.00 kg')
    expect(row).toHaveTextContent(/buyer/i)
    expect(row).not.toHaveTextContent(/^Quantity/)
  })

  test('the offered total is this opportunity own, and says the database sums it', () => {
    loaded()
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('offered-total')).toHaveTextContent('6,400.00 kg')
    expect(screen.getByTestId('offered-total-note')).toHaveTextContent(/re-summed|database/i)
  })

  test('an opportunity is not a sale, on this screen too', () => {
    loaded()
    render(<OpportunityDetailScreen />)
    expect(screen.getByText(/not a sale/i)).toBeInTheDocument()
  })
})

describe('status controls follow the machine', () => {
  test('a proposed opportunity can be shared, declined or lapsed', () => {
    loaded({ status: 'proposed' })
    render(<OpportunityDetailScreen />)

    const actions = screen.getByTestId('opportunity-actions')
    expect(within(actions).getByTestId('action-share')).toBeInTheDocument()
    expect(within(actions).getByTestId('action-decline')).toBeInTheDocument()
    expect(within(actions).getByTestId('action-lapse')).toBeInTheDocument()
    // Accepted describes the buyer's answer. Nobody has shown them anything.
    expect(within(actions).queryByTestId('action-accept')).not.toBeInTheDocument()
  })

  test('a shared opportunity can be accepted', () => {
    loaded({ status: 'shared' })
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('action-accept')).toBeInTheDocument()
    expect(screen.queryByTestId('action-share')).not.toBeInTheDocument()
  })

  test('an accepted opportunity can still be declined or lapsed', () => {
    loaded({ status: 'accepted' })
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('action-decline')).toBeInTheDocument()
    expect(screen.getByTestId('action-lapse')).toBeInTheDocument()
    expect(screen.queryByTestId('action-accept')).not.toBeInTheDocument()
  })

  test('a released opportunity offers no controls at all', () => {
    for (const status of ['declined', 'lapsed'] as const) {
      loaded({ status })
      const { unmount } = render(<OpportunityDetailScreen />)
      expect(screen.queryByTestId('opportunity-actions')).not.toBeInTheDocument()
      unmount()
    }
  })

  // A forward move costs nothing and undoes nothing.
  test('sharing goes straight through', () => {
    loaded({ status: 'proposed' })
    render(<OpportunityDetailScreen />)

    fireEvent.click(screen.getByTestId('action-share'))
    expect(statusMutate).toHaveBeenCalledWith('shared')
  })
})

/**
 * Declining and lapsing are one-way AND they move other people's numbers:
 * per business-rules §7 the supply returns to `v_harvest_available`. Both
 * facts belong in front of the user before the click, not after.
 */
describe('releasing supply is confirmed first', () => {
  test('declining asks first and does not fire the mutation', () => {
    loaded({ status: 'shared' })
    render(<OpportunityDetailScreen />)

    fireEvent.click(screen.getByTestId('action-decline'))

    expect(statusMutate).not.toHaveBeenCalled()
    expect(screen.getByTestId('release-confirm')).toBeInTheDocument()
  })

  test('the confirmation says the supply comes back and the move cannot be undone', () => {
    loaded({ status: 'shared' })
    render(<OpportunityDetailScreen />)

    fireEvent.click(screen.getByTestId('action-decline'))
    const confirm = screen.getByTestId('release-confirm')

    expect(confirm).toHaveTextContent(/available/i)
    expect(confirm).toHaveTextContent(/cannot be undone|final/i)
  })

  test('confirming performs the transition', () => {
    loaded({ status: 'shared' })
    render(<OpportunityDetailScreen />)

    fireEvent.click(screen.getByTestId('action-decline'))
    fireEvent.click(screen.getByTestId('release-confirm-yes'))

    expect(statusMutate).toHaveBeenCalledWith('declined')
  })

  test('lapsing is confirmed the same way, and targets lapsed', () => {
    loaded({ status: 'proposed' })
    render(<OpportunityDetailScreen />)

    fireEvent.click(screen.getByTestId('action-lapse'))
    fireEvent.click(screen.getByTestId('release-confirm-yes'))

    expect(statusMutate).toHaveBeenCalledWith('lapsed')
  })

  test('cancelling leaves the opportunity alone', () => {
    loaded({ status: 'shared' })
    render(<OpportunityDetailScreen />)

    fireEvent.click(screen.getByTestId('action-decline'))
    fireEvent.click(screen.getByTestId('release-confirm-no'))

    expect(statusMutate).not.toHaveBeenCalled()
    expect(screen.queryByTestId('release-confirm')).not.toBeInTheDocument()
  })

  // Four buttons all reading "Saving…" would be four claims about one write.
  test('a transition in flight disables the controls and says so once', () => {
    loaded({ status: 'proposed' })
    useOpportunityStatus.mockReturnValue({
      mutate: statusMutate,
      isPending: true,
      isError: false,
      error: null,
      reset: vi.fn(),
    })
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('action-share')).toBeDisabled()
    expect(screen.getByTestId('action-decline')).toBeDisabled()
    expect(screen.getByTestId('action-share')).toHaveTextContent(/share/i)
    expect(screen.getAllByTestId('status-saving')).toHaveLength(1)
  })

  // An RLS refusal on a WRITE means the UI offered a control it should not
  // have — §9 calls that a bug to fix. The user still needs a sentence, and
  // `new row violates row-level security policy` is not one.
  test('a failed transition is reported in words, not in policy language', () => {
    loaded({ status: 'shared' })
    useOpportunityStatus.mockReturnValue({
      mutate: statusMutate,
      isPending: false,
      isError: true,
      error: new Error('new row violates row-level security policy'),
      reset: vi.fn(),
    })
    render(<OpportunityDetailScreen />)

    const shown = screen.getByTestId('status-error')
    expect(shown).toHaveTextContent(/do not have permission/i)
    expect(shown).not.toHaveTextContent(/row-level security/i)
  })
})

/**
 * QA #12, first bullet, answered by the schema rather than by a delete:
 * `opportunity_supply` has no DELETE policy and no `deleted_at`. §7's release
 * IS the mechanism, so a released opportunity must say so — otherwise two
 * supply lines still sit on screen looking committed.
 */
/**
 * Picking a harvest is picking a farmer, and a line cannot be detached, so
 * each line says who attached it and when (20261001090001).
 */
describe('each supply line says who attached it', () => {
  test('the attacher and the date are on the line', () => {
    loaded()
    render(<OpportunityDetailScreen />)

    const row = screen.getByTestId('supply-row')
    expect(within(row).getByTestId('supply-attached-by')).toHaveTextContent('Fixture Ops')
    expect(within(row).getByTestId('supply-attached-by')).toHaveTextContent('1 Oct 2026')
    expect(screen.getByRole('columnheader', { name: 'Attached by' })).toBeInTheDocument()
  })

  // Seeded lines were written with no signed-in actor, and a name the caller
  // may not see is zero rows: both are a dash, never a raw id.
  test('no recorded attacher, or a name not returned, is a dash', () => {
    loaded({
      supply: [
        { ...opportunity().supply[0], captured_by: null },
        { ...opportunity().supply[0], harvest_report_id: 'h2', captured_by: 'u-hidden' },
      ],
    })
    render(<OpportunityDetailScreen />)

    const cells = screen.getAllByTestId('supply-attached-by')
    expect(cells[0]).toHaveTextContent(/^—/)
    expect(cells[1]).toHaveTextContent(/^—/)
    expect(cells[1]).not.toHaveTextContent('u-hidden')
  })
})

describe('a released opportunity explains itself', () => {
  test('declined says the supply went back to available', () => {
    loaded({ status: 'declined' })
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('released-note')).toHaveTextContent(/available/i)
  })

  test('its supply lines are still listed — the record is not erased', () => {
    loaded({ status: 'declined' })
    render(<OpportunityDetailScreen />)

    expect(screen.getAllByTestId('supply-row')).toHaveLength(1)
  })

  // Attaching to a released opportunity would add a line that counts toward
  // nothing: committed_kg only sums the live statuses.
  test('nothing more can be attached to it', () => {
    loaded({ status: 'declined' })
    render(<OpportunityDetailScreen />)

    expect(screen.queryByTestId('attach-submit')).not.toBeInTheDocument()
    expect(screen.getByTestId('attach-closed')).toBeInTheDocument()
  })

  test('a live opportunity still offers the attach form', () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({
      isLoading: false,
      data: [
        {
          harvest_report_id: 'h9',
          crop_cycle_id: 'cy9',
          quantity_kg: 4100,
          available_kg: 4100,
          harvest_start: '2026-09-01',
        },
      ],
    })
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('attach-submit')).toBeInTheDocument()
    expect(screen.queryByTestId('attach-closed')).not.toBeInTheDocument()
  })
})

/**
 * QA #12, fourth bullet: the option read "4,100.00 kg · 0.00 kg Available ·
 * 1 Sep 2026" — a capital "Available" mid-phrase, and a leading figure with no
 * label at all on a screen that exists to keep two quantities apart.
 */
describe('the harvest options name their figures', () => {
  const harvest = (over: Record<string, unknown> = {}) => ({
    harvest_report_id: 'h9',
    crop_cycle_id: 'cy9',
    quantity_kg: 4100,
    available_kg: 2000,
    harvest_start: '2026-09-10',
    farmer: 'Joseph Kalinga',
    plot_label: 'Shamba la mto',
    ...over,
  })
  const openOption = async () => {
    await userEvent.click(screen.getByTestId('attach-harvest'))
    return screen.findByRole('option', { name: /4,100/ })
  }
  /** The label/value pairs of one option, in order. */
  const fields = (option: HTMLElement) =>
    within(option)
      .getAllByTestId('harvest-option-field')
      .map((row) => row.textContent?.replace(/\s+/g, ' ').trim())

  // Picking an option is picking a farmer, and Attach cannot be undone, so the
  // option says whose harvest it is before the choice, not after. Every figure
  // carries its own label: an unlabelled number on this screen is exactly the
  // ambiguity the rest of it exists to avoid.
  test('each option is grouped: farmer, then labelled plot, figures and harvest date', async () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({ isLoading: false, data: [harvest()] })
    render(<OpportunityDetailScreen />)

    const option = await openOption()
    expect(within(option).getByTestId('harvest-option-farmer')).toHaveTextContent('Joseph Kalinga')
    expect(fields(option)).toEqual([
      'Plot Shamba la mto',
      'Expected 4,100.00 kg',
      'Available 2,000.00 kg',
      'Harvest from 10 Sep 2026',
    ])
  })

  test('a harvest with no start date has no harvest row', async () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({ isLoading: false, data: [harvest({ harvest_start: null })] })
    render(<OpportunityDetailScreen />)

    expect(fields(await openOption())).toEqual(['Plot Shamba la mto', 'Expected 4,100.00 kg', 'Available 2,000.00 kg'])
  })

  // Zero rows is an answer: a name RLS does not return is shown as unknown,
  // and the figure is still offered.
  test('a name that did not come back is a dash, not a missing option', async () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({
      isLoading: false,
      data: [harvest({ farmer: null, plot_label: null })],
    })
    render(<OpportunityDetailScreen />)

    const option = await openOption()
    expect(within(option).getByTestId('harvest-option-farmer')).toHaveTextContent('—')
    expect(fields(option)[0]).toBe('Plot —')
  })

  // The chosen figure stays readable in the closed control: who, then both
  // quantities named, in sentence case (QA #12).
  test('the chosen harvest is summarised in the closed control', async () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({ isLoading: false, data: [harvest()] })
    render(<OpportunityDetailScreen />)

    await userEvent.click(await openOption())
    const trigger = screen.getByTestId('attach-harvest')
    expect(within(trigger).getByTestId('harvest-chosen-farmer')).toHaveTextContent('Joseph Kalinga · Shamba la mto')
    expect(trigger).toHaveTextContent('4,100.00 kg expected · 2,000.00 kg available · harvest from 10 Sep 2026')
  })

  // Fully committed figures stay listed: hiding them would be a client-side
  // copy of opportunity_supply_guard.
  test('a fully committed figure is still offered', async () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({
      isLoading: false,
      data: [
        { harvest_report_id: 'h9', crop_cycle_id: 'cy9', quantity_kg: 4100, available_kg: 0, harvest_start: null },
      ],
    })
    render(<OpportunityDetailScreen />)

    await userEvent.click(screen.getByTestId('attach-harvest'))
    expect(await screen.findAllByRole('option')).toHaveLength(1)
  })
})

/**
 * QA #21's tail. `opportunity_supply.contributed_kg` is
 * `check (contributed_kg > 0)`, so 0 or a negative came back as a raw
 * constraint name — the same family as the harvest window and the request
 * hours.
 */
describe('the contribution has to be a contribution', () => {
  const withHarvest = () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({
      isLoading: false,
      data: [
        {
          harvest_report_id: 'h9',
          crop_cycle_id: 'cy9',
          quantity_kg: 4100,
          available_kg: 4100,
          harvest_start: '2026-09-01',
        },
      ],
    })
  }

  const attach = async (kg: string, harvest = 'h9') => {
    render(<OpportunityDetailScreen />)
    if (harvest) await chooseHarvest()
    fireEvent.change(screen.getByTestId('attach-kg'), { target: { value: kg } })
    fireEvent.click(screen.getByTestId('attach-submit'))
  }

  test('zero kilograms is refused inline, not by the database', async () => {
    withHarvest()
    await attach('0')

    expect(screen.getByTestId('attach-kg-error')).toHaveTextContent(/more than zero/i)
    expect(attachMutateAsync).not.toHaveBeenCalled()
  })

  test('a negative contribution is refused', async () => {
    withHarvest()
    await attach('-5')

    expect(screen.getByTestId('attach-kg-error')).toBeInTheDocument()
    expect(attachMutateAsync).not.toHaveBeenCalled()
  })

  test('text is refused, and says what is wrong', async () => {
    withHarvest()
    await attach('abc')

    expect(screen.getByTestId('attach-kg-error')).toHaveTextContent(/enter a number/i)
  })

  test('a blank says which answer is missing rather than nothing happening', async () => {
    withHarvest()
    await attach('')

    expect(screen.getByTestId('attach-kg-error')).toBeInTheDocument()
    expect(attachMutateAsync).not.toHaveBeenCalled()
  })

  test('choosing no harvest figure says so', async () => {
    withHarvest()
    await attach('100', '')

    expect(screen.getByTestId('attach-harvest-error')).toBeInTheDocument()
    expect(attachMutateAsync).not.toHaveBeenCalled()
  })

  test('a valid contribution goes through', async () => {
    withHarvest()
    await attach('1600')

    // The second argument is the per-call `onSettled` that releases the
    // in-flight latch, so the payload is asserted on its own.
    expect(attachMutateAsync.mock.calls[0][0]).toEqual({
      id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      harvestReportId: 'h9',
      cropCycleId: 'cy9',
      contributedKg: 1600,
    })
  })

  /**
   * The important one. A figure past what is available must still be SENT, so
   * that `opportunity_supply_guard` refuses it and its message — which names
   * the real kilograms — is what the user reads. A client-side copy of that
   * check would drift and would be wrong the moment another opportunity
   * committed the same harvest.
   */
  test('a quantity beyond what is available is still sent to the guard', async () => {
    withHarvest()
    await attach('99999')

    expect(attachMutateAsync.mock.calls[0][0]).toMatchObject({ contributedKg: 99999 })
  })

  test('the error clears once the figure is fixed', async () => {
    withHarvest()
    render(<OpportunityDetailScreen />)
    await chooseHarvest()
    fireEvent.change(screen.getByTestId('attach-kg'), { target: { value: '0' } })
    fireEvent.click(screen.getByTestId('attach-submit'))
    expect(screen.getByTestId('attach-kg-error')).toBeInTheDocument()

    fireEvent.change(screen.getByTestId('attach-kg'), { target: { value: '1600' } })
    fireEvent.click(screen.getByTestId('attach-submit'))

    expect(screen.queryByTestId('attach-kg-error')).not.toBeInTheDocument()
    expect(attachMutateAsync).toHaveBeenCalledTimes(1)
  })
})

/**
 * QA #11. Every action outside RegisterScreen was a bare
 * `<button type="button">` with an onClick, so pressing Enter in a text field
 * did nothing and a keyboard user had to tab to the button. On a
 * field-then-submit form that is an accessibility gap, not a style preference.
 */
describe('attaching supply is a real form', () => {
  const withHarvest = () => {
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({
      isLoading: false,
      data: [
        {
          harvest_report_id: 'h9',
          crop_cycle_id: 'cy9',
          quantity_kg: 4100,
          available_kg: 4100,
          harvest_start: '2026-09-01',
        },
      ],
    })
  }

  test('the control submits the form rather than handling a click', () => {
    withHarvest()
    render(<OpportunityDetailScreen />)

    const button = screen.getByTestId('attach-submit')
    expect(button).toHaveAttribute('type', 'submit')
    expect(button.closest('form')).not.toBeNull()
  })

  test('so submitting the form attaches, without touching the button', async () => {
    withHarvest()
    render(<OpportunityDetailScreen />)

    await chooseHarvest()
    fireEvent.change(screen.getByTestId('attach-kg'), { target: { value: '1600' } })
    fireEvent.submit(screen.getByTestId('attach-submit').closest('form')!)

    expect(attachMutateAsync.mock.calls[0][0]).toMatchObject({ contributedKg: 1600 })
  })

  // QA #23. A second submit while the first is in flight is a duplicate write,
  // and the officer's only feedback on a slow connection is that nothing has
  // happened yet.
  test('a submit in flight disables the control', () => {
    withHarvest()
    useAttachSupply.mockReturnValue({
      // No `mutate`: its per-call callbacks are skipped after unmount.
    mutateAsync: attachMutateAsync,
      isPending: true,
      isError: false,
      error: null,
      reset: vi.fn(),
    })
    render(<OpportunityDetailScreen />)

    expect(screen.getByTestId('attach-submit')).toBeDisabled()
  })

  test('and a second submit in the same tick does not fire twice', async () => {
    withHarvest()
    render(<OpportunityDetailScreen />)

    await chooseHarvest()
    fireEvent.change(screen.getByTestId('attach-kg'), { target: { value: '1600' } })
    const form = screen.getByTestId('attach-submit').closest('form')!
    fireEvent.submit(form)
    fireEvent.submit(form)

    expect(attachMutateAsync).toHaveBeenCalledTimes(1)
  })
})

describe('the supply draft after a confirmed save', () => {
  test('is cleared even when the screen unmounted before the save resolved', async () => {
    const key = 'form:u1:o1:opportunity-supply'
    session.data = { userId: 'u1' }
    let resolve!: () => void
    attachMutateAsync.mockReturnValue(new Promise<void>((r) => { resolve = r }))
    loaded({ status: 'proposed' })
    useAvailableHarvest.mockReturnValue({
      isLoading: false,
      data: [{ harvest_report_id: 'h9', crop_cycle_id: 'cy9', quantity_kg: 4100, available_kg: 4100, harvest_start: '2026-09-01' }],
    })

    const { unmount } = render(<OpportunityDetailScreen />)
    await waitFor(() => expect(screen.getByTestId('attach-submit')).toBeEnabled())
    await chooseHarvest()
    fireEvent.change(screen.getByTestId('attach-kg'), { target: { value: '1600' } })
    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toMatchObject({ values: { contributed_kg: '1600' } }))

    fireEvent.submit(screen.getByTestId('attach-submit').closest('form')!)
    expect(attachMutateAsync).toHaveBeenCalledTimes(1)
    unmount()
    await act(async () => resolve())

    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toBeUndefined())
  })
})
