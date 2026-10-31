import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useDemands = vi.fn()
const useDemandFormOptions = vi.fn()
const mutateAsync = vi.fn()
const createState = { isPending: false, error: null as Error | null }

vi.mock('@/features/ops/useDemand', () => ({
  useDemands: () => useDemands(),
  useDemandFormOptions: () => useDemandFormOptions(),
  // No `mutate`: its per-call callbacks are skipped after unmount.
  useCreateDemand: () => ({ mutateAsync, reset: vi.fn(), isError: false, ...createState }),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => ({}), useNavigate: () => vi.fn() }),
  useNavigate: () => vi.fn(),
  Link: ({ children }: { children: React.ReactNode }) => <a href="#x">{children}</a>,
}))
// No userId by default: the draft stays in memory. One test signs in.
const session = {
  data: {
    userId: undefined as string | undefined,
    memberships: [
      {
        id: 'm1',
        role: 'ops',
        project_id: '20000000-0000-4000-8000-000000000001',
        village_id: null,
        revoked_at: null,
      },
    ],
  },
}
vi.mock('@/app/session', () => ({ useSession: () => session }))
// In-memory in place of IndexedDB, so a stored draft can be inspected.
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})

const { DemandListScreen } = await import('@/features/ops/DemandListScreen')
const { indexedDbDraftStore } = await import('@/lib/drafts')
await import('@/i18n')

beforeEach(() => {
  useDemands.mockReset()
  useDemandFormOptions.mockReset()
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue({ id: 'd1' })
  session.data.userId = undefined
  createState.isPending = false
  createState.error = null

  // `useDemands` returns the SHAPED rows under `demands`, not raw `data`.
  useDemands.mockReturnValue({ isLoading: false, error: null, demands: [] })
  useDemandFormOptions.mockReturnValue({
    isLoading: false,
    data: {
      buyers: [
        {
          id: 'b1',
          name: 'Iringa Grain Traders',
          project_id: '20000000-0000-4000-8000-000000000001',
        },
      ],
      crops: [{ id: 'c1', name: 'Maize' }],
    },
  })
})

const choose = async (testId: string, label: string) => {
  await userEvent.click(screen.getByTestId(testId))
  await userEvent.click(await screen.findByRole('option', { name: label }))
}

const openCreate = async () => {
  await userEvent.click(screen.getByTestId('demand-create-open'))
}

const fill = async () => {
  await openCreate()
  await choose('demand-buyer', 'Iringa Grain Traders')
  await choose('demand-crop', 'Maize')
  fireEvent.change(screen.getByTestId('demand-quantity'), { target: { value: '2500' } })
}

/**
 * QA #11 and #23 — spec 7.6's create form.
 *
 * It was a bare `<button type="button">` with an onClick, on a form whose
 * fields are typed then submitted. A keyboard user had to tab past eight
 * fields to reach it, and Enter did nothing.
 */
describe('the demand create form', () => {
  test('stays collapsed until the create action is requested', () => {
    render(<DemandListScreen />)

    expect(screen.queryByTestId('demand-create-panel')).not.toBeInTheDocument()
    expect(screen.getByTestId('demand-create-open')).toHaveAttribute('aria-expanded', 'false')
  })

  test('opens the full-width form from the page action', async () => {
    render(<DemandListScreen />)
    await openCreate()

    expect(screen.getByTestId('demand-create-panel')).toHaveClass('w-full')
    expect(screen.getByTestId('demand-create-open')).toHaveAttribute('aria-expanded', 'true')
  })

  test('submits as a form, not as a click handler', async () => {
    render(<DemandListScreen />)
    await openCreate()

    const button = screen.getByTestId('demand-create-submit')
    expect(button).toHaveAttribute('type', 'submit')
    expect(button.closest('form')).not.toBeNull()
  })

  test('so submitting the form records the demand', async () => {
    render(<DemandListScreen />)
    await fill()

    fireEvent.submit(screen.getByTestId('demand-create-submit').closest('form')!)

    expect(mutateAsync).toHaveBeenCalledTimes(1)
  })

  // The required-field checks still run on the form's own submit, not only on
  // a click.
  test('and an empty form still reports what is missing', async () => {
    render(<DemandListScreen />)
    await openCreate()

    fireEvent.submit(screen.getByTestId('demand-create-submit').closest('form')!)

    expect(screen.getByTestId('demand-buyer-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a write in flight disables the control', async () => {
    createState.isPending = true
    render(<DemandListScreen />)
    await openCreate()

    expect(screen.getByTestId('demand-create-submit')).toBeDisabled()
  })

  // QA #23: the data was safe, but three submits in one tick is three writes.
  test('a second submit in the same tick does not write twice', async () => {
    render(<DemandListScreen />)
    await fill()

    const form = screen.getByTestId('demand-create-submit').closest('form')!
    fireEvent.submit(form)
    fireEvent.submit(form)

    expect(mutateAsync).toHaveBeenCalledTimes(1)
  })
})

describe('the draft after a confirmed save', () => {
  test('is cleared even when the screen unmounted before the save resolved', async () => {
    const key = 'form:u1:20000000-0000-4000-8000-000000000001:demand-create'
    session.data.userId = 'u1'
    let resolve!: (value: { id: string }) => void
    mutateAsync.mockReturnValue(new Promise((r) => { resolve = r }))

    const { unmount } = render(<DemandListScreen />)
    await fill()
    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toMatchObject({ values: { quantity: '2500' } }))

    fireEvent.submit(screen.getByTestId('demand-create-submit').closest('form')!)
    expect(mutateAsync).toHaveBeenCalledTimes(1)
    unmount()
    await act(async () => resolve({ id: 'd1' }))

    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toBeUndefined())
  })
})

/**
 * `quantity_kg` and `indicative_price_per_kg` are `numeric(12,2)`, which
 * rounds a third decimal silently. And the price travels as text: `'12,5'` or
 * `'abc'` became `Number(...) = NaN`, was sent as null, and the price the user
 * typed simply vanished. The form refuses both rather than guessing.
 */
describe('the figures the columns would silently change', () => {
  const submitForm = () =>
    fireEvent.submit(screen.getByTestId('demand-create-submit').closest('form')!)
  const setField = (testId: string, value: string) =>
    fireEvent.change(screen.getByTestId(testId), { target: { value } })

  test('a price written with a comma is refused, not dropped', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-price', '12,5')

    submitForm()

    expect(screen.getByTestId('demand-price-error')).toHaveTextContent(/point/i)
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a price that is not a number is refused', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-price', 'abc')

    submitForm()

    expect(screen.getByTestId('demand-price-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  // The check constraint owns this rule; the client sends it and shows the reply.
  test('a negative price is sent to the database, not pre-checked', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-price', '-1')

    submitForm()

    expect(screen.queryByTestId('demand-price-error')).not.toBeInTheDocument()
    expect(mutateAsync).toHaveBeenCalledTimes(1)
  })

  test('a price with three decimals is refused', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-price', '12.555')

    submitForm()

    expect(screen.getByTestId('demand-price-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a price written with a point passes through as typed', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-price', '12.5')

    submitForm()

    expect(screen.queryByTestId('demand-price-error')).not.toBeInTheDocument()
    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({ pricePerKg: '12.5', quantityKg: 2500 })
  })

  test('the price is optional', async () => {
    render(<DemandListScreen />)
    await fill()

    submitForm()

    expect(screen.queryByTestId('demand-price-error')).not.toBeInTheDocument()
    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({ pricePerKg: '' })
  })

  test('a quantity with three decimals is refused', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-quantity', '10.123')

    submitForm()

    expect(screen.getByTestId('demand-quantity-error')).toHaveTextContent(/decimal/i)
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a quantity with two decimals passes', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-quantity', '10.12')

    submitForm()

    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({ quantityKg: 10.12 })
  })

  // `Number('abc') <= 0` is false, so text used to pass the old check and be
  // sent as NaN.
  test('a quantity that is not a number is refused', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-quantity', 'abc')

    submitForm()

    expect(screen.getByTestId('demand-quantity-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  // The `touched` gate still holds: nothing is flagged before a submit.
  test('no figure is flagged before the first submit', async () => {
    render(<DemandListScreen />)
    await fill()
    setField('demand-price', '12,5')
    setField('demand-quantity', '10.123')

    expect(screen.queryByTestId('demand-price-error')).not.toBeInTheDocument()
    expect(screen.queryByTestId('demand-quantity-error')).not.toBeInTheDocument()
  })
})
