import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useBuyers = vi.fn()
const mutateAsync = vi.fn()
const createState = { isPending: false, error: null as unknown, isSuccess: false }

vi.mock('@/features/ops/useOpsReference', () => ({
  useBuyers: () => useBuyers(),
  // No `mutate`: its per-call callbacks are skipped after unmount.
  useCreateBuyer: () => ({ mutateAsync, ...createState }),
  BUYER_CHANNELS: ['direct', 'afm', 'other'],
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

const { BuyersScreen } = await import('@/features/ops/BuyersScreen')
const { indexedDbDraftStore } = await import('@/lib/drafts')
await import('@/i18n')

beforeEach(() => {
  useBuyers.mockReset()
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue(undefined)
  session.data.userId = undefined
  createState.isPending = false
  createState.error = null
  createState.isSuccess = false
})

async function chooseChannel(label: string) {
  await userEvent.click(screen.getByTestId('buyer-channel'))
  await userEvent.click(await screen.findByRole('option', { name: label }))
}

async function openCreate() {
  await userEvent.click(screen.getByTestId('buyer-create-open'))
}

const buyer = (over: Record<string, unknown> = {}) => ({
  id: 'b1',
  project_id: '20000000-0000-4000-8000-000000000001',
  name: 'Iringa Grain Traders — DEMO',
  channel: 'direct',
  contact_note: 'Collects from the shed',
  is_active: true,
  ...over,
})

/** Spec 7.5 — buyer list and create. */
describe('BuyersScreen states', () => {
  test('loading shows a loading state', () => {
    useBuyers.mockReturnValue({ isLoading: true, error: null, data: [] })
    render(<BuyersScreen />)

    expect(screen.getByTestId('buyers-loading')).toBeInTheDocument()
  })

  test('an error is an error', () => {
    useBuyers.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: [],
      refetch: vi.fn(),
    })
    render(<BuyersScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })

  test('no buyers is an empty state, not an error', () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('BuyersScreen content', () => {
  test('lists buyers with channel and contact note', () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [buyer()] })
    render(<BuyersScreen />)

    const table = screen.getByTestId('buyers-table')
    expect(table).toHaveTextContent('Iringa Grain Traders')
    expect(table).toHaveTextContent('Direct')
    expect(table).toHaveTextContent('Collects from the shed')
  })

  // "channel = 'afm' is a label only; no integration." The screen must say so
  // rather than let a reader infer a partnership that does not exist.
  test('states that the channel is a label, with no integration implied', () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [buyer()] })
    render(<BuyersScreen />)

    expect(screen.getByTestId('buyers-note')).toHaveTextContent(/no integration/i)
  })
})

describe('BuyersScreen create', () => {
  test('the create form is collapsed until requested', () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)

    expect(screen.queryByTestId('buyer-create-panel')).not.toBeInTheDocument()
    expect(screen.getByTestId('buyer-create-open')).toHaveAttribute('aria-expanded', 'false')
  })

  test('a nameless buyer is refused before a request is made', async () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    await userEvent.click(screen.getByTestId('buyer-create-submit'))

    expect(screen.getByTestId('buyer-name-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a whitespace-only name is not a name', async () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    await userEvent.type(screen.getByTestId('buyer-name'), '   ')
    await userEvent.click(screen.getByTestId('buyer-create-submit'))

    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a valid buyer is sent with its draft id and trimmed values', async () => {
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    await userEvent.type(screen.getByTestId('buyer-name'), '  Mbeya Millers  ')
    await chooseChannel('AFM')
    await userEvent.click(screen.getByTestId('buyer-create-submit'))

    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toEqual({
      // The draft's clientRef, so a retry after a lost reply replays instead of adding twice.
      id: expect.stringMatching(/^[0-9a-f-]{36}$/),
      name: 'Mbeya Millers',
      channel: 'afm',
      contact_note: null,
    })
  })

  // The unique (project_id, name) constraint belongs to the database, and is
  // called rather than pre-checked (business-rules §0.3). What reaches the
  // user is the collision in words: Postgres names the CONSTRAINT, not the
  // value, so `duplicate key value violates unique constraint
  // "buyer_project_id_name_key"` is not the sentence anybody needs (QA #4).
  test('a database refusal is surfaced rather than swallowed', async () => {
    createState.error = new Error(
      'duplicate key value violates unique constraint "buyer_project_id_name_key"',
    )
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    const shown = screen.getByTestId('error-state')
    expect(shown).toHaveTextContent(/buyer with that name already exists/i)
    expect(shown).not.toHaveTextContent('buyer_project_id_name_key')
  })

  test("a 422 naming the name is shown at the name, not in the banner", async () => {
    createState.error = { isAxiosError: true, response: { status: 422, data: { message: 'invalid', errors: { name: ['a buyer may have at most 120 characters'] } } } }
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    expect(await screen.findByTestId('buyer-name-error')).toHaveTextContent('a buyer may have at most 120 characters')
    expect(screen.queryByTestId('buyer-create-error')).not.toBeInTheDocument()
  })

  test('a saved buyer says so', async () => {
    createState.isSuccess = true
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    expect(within(screen.getByTestId('buyer-saved')).getByRole('status')).toHaveTextContent('Buyer added')
  })

  test('submit is disabled while the write is in flight', async () => {
    createState.isPending = true
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<BuyersScreen />)
    await openCreate()

    expect(screen.getByTestId('buyer-create-submit')).toBeDisabled()
  })
})

describe('the buyer draft after a confirmed save', () => {
  test('is cleared even when the screen unmounted before the save resolved', async () => {
    const key = 'form:u1:20000000-0000-4000-8000-000000000001:buyer-create'
    session.data.userId = 'u1'
    let resolve!: () => void
    mutateAsync.mockReturnValue(new Promise<void>((r) => { resolve = r }))
    useBuyers.mockReturnValue({ isLoading: false, error: null, data: [] })

    const { unmount } = render(<BuyersScreen />)
    await openCreate()
    await userEvent.type(screen.getByTestId('buyer-name'), 'Mbeya Millers')
    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toMatchObject({ values: { name: 'Mbeya Millers' } }))

    await userEvent.click(screen.getByTestId('buyer-create-submit'))
    await waitFor(() => expect(mutateAsync).toHaveBeenCalledTimes(1))
    unmount()
    await act(async () => resolve())

    await waitFor(async () => expect(await indexedDbDraftStore.get(key)).toBeUndefined())
  })
})
