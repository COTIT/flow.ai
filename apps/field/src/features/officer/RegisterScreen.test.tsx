import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

const useSession = vi.fn()
const useCrops = vi.fn()
const useDraft = vi.fn()
const rpc = vi.fn()
const navigate = vi.fn()
const save = vi.fn()
const clear = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => ({ draft: 'draft-ref-1' }) }),
  useNavigate: () => navigate,
  Link: ({ children }: { children: React.ReactNode }) => <a href="#x">{children}</a>,
}))
vi.mock('@/app/session', () => ({ useSession: () => useSession() }))
vi.mock('@/features/officer/useCrops', () => ({ useCrops: () => useCrops() }))
const useDraftArgs: unknown[][] = []
// The picked title documents keep their own draft, beside the form's.
const docsDraft = vi.fn()
const docsSave = vi.fn()
const docsClear = vi.fn()
vi.mock('@/lib/drafts', () => ({
  draftKey: (a: string, b: string) => `${a}:${b}`,
  indexedDbDraftStore: { get: vi.fn(), set: vi.fn(), clear: vi.fn() },
  useDraft: (...args: unknown[]) => {
    useDraftArgs.push(args)
    const form = useDraft()
    const docs = docsDraft()
    return String(args[0]).startsWith('register-docs:') ? docs : form
  },
}))
const uploadPlotDocuments = vi.fn()
vi.mock('@/lib/plotDocuments', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/plotDocuments')>()),
  uploadPlotDocuments: (...args: unknown[]) => uploadPlotDocuments(...args),
}))
vi.mock('@/lib/supabase', () => ({ supabase: { rpc: (...args: unknown[]) => rpc(...args) } }))
// The login card has its own tests; here only WHERE and FOR WHOM it appears.
vi.mock('@/features/officer/FarmerLoginCard', () => ({
  FarmerLoginCard: ({ personId, autoIssue }: { personId: string; autoIssue?: boolean }) => (
    <div data-testid="farmer-login-card" data-person={personId} data-auto={String(Boolean(autoIssue))} />
  ),
}))

const { RegisterScreen } = await import('@/features/officer/RegisterScreen')
const { isRegisterDraft } = await import('@/features/officer/registerSchema')
await import('@/i18n')

/** A shape-valid empty draft, for the restore tests. */
const EMPTY_DRAFT = {
  given_name: '',
  family_name: '',
  phone: '',
  household_label: '',
  is_head: true,
  farm_label: '',
  farm_latitude: '',
  farm_longitude: '',
  plot_label: '',
  plot_area_ha: '',
  season_label: '',
  planted_on: '',
  cycles: [],
  confidence: 'medium',
}

const VILLAGE = '30000000-0000-4000-8000-000000000001'
const MAIZE = '40000000-0000-4000-8000-000000000001'
const COFFEE = '40000000-0000-4000-8000-000000000003'

beforeEach(() => {
  rpc.mockReset()
  navigate.mockReset()
  save.mockReset()
  clear.mockReset()
  useSession.mockReturnValue({
    isLoading: false,
    error: null,
    data: {
      appUser: { id: 'u1', person_id: null },
      memberships: [
        { id: 'm1', role: 'field_officer', project_id: 'p1', village_id: VILLAGE, revoked_at: null },
      ],
    },
  })
  useCrops.mockReturnValue({
    isLoading: false,
    error: null,
    crops: [
      { id: MAIZE, name: 'Mahindi', measured_by: 'area' },
      { id: COFFEE, name: 'Kahawa', measured_by: 'tree_count' },
    ],
  })
  useDraft.mockReturnValue({ status: 'clean', draft: null, save, clear })
  rpc.mockResolvedValue({ data: { person_id: 'new-person' }, error: null })
  docsSave.mockReset()
  docsClear.mockReset()
  docsDraft.mockReturnValue({ status: 'empty', draft: undefined, save: docsSave, clear: docsClear })
  uploadPlotDocuments.mockReset()
  uploadPlotDocuments.mockResolvedValue({ filed: [], failed: [] })
})

function renderScreen() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  return render(
    <QueryClientProvider client={client}>
      <RegisterScreen />
    </QueryClientProvider>,
  )
}

/** Ticks (or unticks) one crop grown on the plot. */
async function chooseCrop(label: string) {
  await userEvent.click(screen.getByRole('checkbox', { name: label }))
}

/** Fills a complete, valid registration. */
async function fillValid() {
  fireEvent.change(screen.getByTestId('register-given-name'), { target: { value: 'Neema' } })
  fireEvent.change(screen.getByTestId('register-family-name'), { target: { value: 'Mwakalinga' } })
  fireEvent.change(screen.getByTestId('register-phone'), { target: { value: '+255700000101' } })
  fireEvent.change(screen.getByTestId('register-farm-label'), { target: { value: 'Shamba' } })
  // jsdom has no GPS, so the location fields stay open for typing — the same
  // fallback an officer gets when the handset refuses a position.
  fireEvent.change(screen.getByTestId('register-farm-latitude'), { target: { value: '-8.1303' } })
  fireEvent.change(screen.getByTestId('register-farm-longitude'), { target: { value: '35.1895' } })
  fireEvent.change(screen.getByTestId('register-plot-label'), { target: { value: 'Kipande' } })
  await chooseCrop('Mahindi')
  fireEvent.change(screen.getByTestId('register-cycle-area'), { target: { value: '1.6' } })
}

const submit = () => fireEvent.click(screen.getByTestId('register-submit'))

/**
 * QA #16 and #17. `required: true` rejected `""` and accepted `"   "`, and
 * `person.given_name` is `not null` — which `'   '` satisfies. A farmer could
 * be registered with a blank name, on a screen with no rename to fix it.
 */
describe('whitespace-only names', () => {
  test('are refused inline, and never reach the RPC', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-given-name'), { target: { value: '   ' } })
    submit()

    await waitFor(() =>
      expect(screen.getByTestId('register-given-name-error')).toBeInTheDocument(),
    )
    expect(rpc).not.toHaveBeenCalled()
  })

  test('and the value that is stored is the trimmed one', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-given-name'), { target: { value: '  Neema  ' } })
    submit()

    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1))
    const payload = rpc.mock.calls[0][1].payload as { person: { given_name: string } }
    expect(payload.person.given_name).toBe('Neema')
  })
})

/**
 * QA #19. The measure field is rendered conditionally on `crop.measured_by`,
 * and was the one field with no check — so the RPC's own prose was doing an
 * inline message's job, one round trip later.
 */
describe('the conditional measure field', () => {
  test('is required for the crop that uses it', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-cycle-area'), { target: { value: '' } })
    submit()

    await waitFor(() =>
      expect(screen.getByTestId('register-cycle-area-ha-error')).toBeInTheDocument(),
    )
    expect(rpc).not.toHaveBeenCalled()
  })

  test('follows the crop: a tree crop asks for trees', async () => {
    renderScreen()
    await fillValid()
    await chooseCrop('Mahindi')
    await chooseCrop('Kahawa')
    submit()

    await waitFor(() =>
      expect(screen.getByTestId('register-cycle-tree-count-error')).toBeInTheDocument(),
    )
    expect(screen.queryByTestId('register-cycle-area-ha-error')).not.toBeInTheDocument()
  })
})

/**
 * QA #21. `cycle_window_sane` caught this and its constraint NAME was rendered
 * as user copy.
 */
describe('the harvest window', () => {
  test('a backwards window is caught inline, on the end date', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-harvest-start'), {
      target: { value: '2026-09-30' },
    })
    fireEvent.change(screen.getByTestId('register-harvest-end'), {
      target: { value: '2026-09-01' },
    })
    submit()

    await waitFor(() =>
      expect(screen.getByTestId('register-harvest-end-error')).toBeInTheDocument(),
    )
    expect(rpc).not.toHaveBeenCalled()
  })

  // A constraint identifier is not user copy — that is the whole point of
  // catching this here.
  test('and says what is wrong in words', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-harvest-start'), {
      target: { value: '2026-09-30' },
    })
    fireEvent.change(screen.getByTestId('register-harvest-end'), {
      target: { value: '2026-09-01' },
    })
    submit()

    const error = await screen.findByTestId('register-harvest-end-error')
    expect(error).toHaveTextContent(/harvest window must end/i)
    expect(error).not.toHaveTextContent(/cycle_window_sane|register\./)
  })
})

/**
 * One message per reason. A single "This is required." under every field was
 * what made #16 and #19 hard to see: the form could only say "something".
 */
describe('the message names the actual problem', () => {
  test('text in a number field says so', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-harvest-kg'), { target: { value: 'abc' } })
    submit()

    const error = await screen.findByTestId('register-harvest-quantity-kg-error')
    expect(error).toHaveTextContent(/enter a number/i)
  })

  test('a number too big for its column says that, rather than overflowing', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-harvest-kg'), {
      target: { value: '999999999999' },
    })
    submit()

    const error = await screen.findByTestId('register-harvest-quantity-kg-error')
    expect(error).toHaveTextContent(/too large/i)
    expect(rpc).not.toHaveBeenCalled()
  })

  test('an empty required field still says it is required', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-family-name'), { target: { value: '' } })
    submit()

    const error = await screen.findByTestId('register-family-name-error')
    expect(error).toHaveTextContent(/required/i)
  })
})

/**
 * QA #28. The phone is now the farmer's login name, so it is required. Its
 * FORMAT is still not checked here: app_normalize_phone decides what a
 * Tanzanian mobile is, and a client copy of that rule would drift.
 */
describe('the phone field', () => {
  test('is required, and never reaches the RPC blank', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-phone'), { target: { value: '  ' } })
    submit()

    await waitFor(() => expect(screen.getByTestId('register-phone-error')).toBeInTheDocument())
    expect(rpc).not.toHaveBeenCalled()
  })

  test('shows the format the programme uses, and leaves checking it to the database', async () => {
    renderScreen()

    expect(screen.getByTestId('register-phone-hint')).toHaveTextContent(/\+255/)

    await fillValid()
    fireEvent.change(screen.getByTestId('register-phone'), { target: { value: '0700 000 101' } })
    submit()

    // Another spelling of a Tanzanian mobile: the database normalises it.
    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1))
  })
})

describe('the farm location', () => {
  test('is required: a registration without it never reaches the RPC', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-farm-latitude'), { target: { value: '' } })
    fireEvent.change(screen.getByTestId('register-farm-longitude'), { target: { value: '' } })
    submit()

    await waitFor(() =>
      expect(screen.getByTestId('register-farm-latitude-error')).toBeInTheDocument(),
    )
    expect(rpc).not.toHaveBeenCalled()
  })
})

/**
 * QA #27. `hectares` is `numeric(10,4)`, so 1.23456789 is stored as 1.2346 —
 * correctly, and silently. The operator typed one number and the record holds
 * another.
 */
describe('silent rounding', () => {
  test('says what will actually be stored', () => {
    renderScreen()
    fireEvent.change(screen.getByTestId('register-plot-area'), {
      target: { value: '1.23456789' },
    })

    expect(screen.getByTestId('register-plot-area-rounded')).toHaveTextContent('1.2346')
  })

  test('says nothing when nothing changes', () => {
    renderScreen()
    fireEvent.change(screen.getByTestId('register-plot-area'), { target: { value: '1.8' } })

    expect(screen.queryByTestId('register-plot-area-rounded')).not.toBeInTheDocument()
  })

  // quantity_kg is numeric(12,2) — a different scale on the same form.
  test('uses each column own scale', async () => {
    renderScreen()
    await chooseCrop('Mahindi')
    fireEvent.change(screen.getByTestId('register-harvest-kg'), { target: { value: '4100.567' } })

    expect(screen.getByTestId('register-harvest-kg-rounded')).toHaveTextContent('4100.57')
  })
})

describe('a valid registration', () => {
  test('is not newly blocked by any of this', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-phone'), { target: { value: '+255700000101' } })
    fireEvent.change(screen.getByTestId('register-plot-area'), { target: { value: '1.8' } })
    fireEvent.change(screen.getByTestId('register-harvest-start'), {
      target: { value: '2026-09-01' },
    })
    fireEvent.change(screen.getByTestId('register-harvest-end'), {
      target: { value: '2026-09-30' },
    })
    fireEvent.change(screen.getByTestId('register-harvest-kg'), { target: { value: '4100' } })
    submit()

    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1))
    expect(rpc.mock.calls[0][0]).toBe('app_register_farmer')
  })

  // The draft id IS the RPC's client_ref, so a retry after a timeout replays
  // rather than creating a second farmer.
  test('sends the draft id as its client_ref', async () => {
    renderScreen()
    await fillValid()
    submit()

    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1))
    const payload = rpc.mock.calls[0][1].payload as { client_ref: string; village_id: string }
    expect(payload.client_ref).toBe('draft-ref-1')
    expect(payload.village_id).toBe(VILLAGE)
  })

  test('clears the draft only after the RPC returned', async () => {
    renderScreen()
    await fillValid()
    submit()

    await waitFor(() => expect(clear).toHaveBeenCalledTimes(1))
  })

  // The farmer's app login is created right away and shown once on this
  // screen: their phone and a temporary password (business-rules §15).
  test('shows the new farmer their app login, created straight away', async () => {
    renderScreen()
    await fillValid()
    submit()

    const card = await screen.findByTestId('farmer-login-card')
    expect(card).toHaveAttribute('data-person', 'new-person')
    expect(card).toHaveAttribute('data-auto', 'true')
  })

  // No person id, no login to create.
  test('a registration with no person returned shows no login card', async () => {
    rpc.mockResolvedValue({ data: {}, error: null })
    renderScreen()
    await fillValid()
    submit()

    await screen.findByTestId('register-success')
    expect(screen.queryByTestId('farmer-login-card')).not.toBeInTheDocument()
  })
})

/**
 * A farmer grows more than one crop, often on the same plot. Each crop ticked
 * becomes its own cycle with its own measure, window and expected harvest.
 */
describe('several crops', () => {
  test('a crop must be chosen before anything is sent', async () => {
    renderScreen()
    await fillValid()
    await chooseCrop('Mahindi')
    submit()

    await waitFor(() => expect(screen.getByTestId('register-cycles-error')).toBeInTheDocument())
    expect(rpc).not.toHaveBeenCalled()
  })

  test('each crop ticked gets its own card, asking for its own measure', async () => {
    renderScreen()
    await chooseCrop('Mahindi')
    await chooseCrop('Kahawa')

    const cards = screen.getAllByTestId('register-cycle-card')
    expect(cards).toHaveLength(2)
    expect(within(cards[0]).getByTestId('register-cycle-area')).toBeInTheDocument()
    expect(within(cards[1]).getByTestId('register-cycle-tree-count')).toBeInTheDocument()
    expect(screen.getAllByTestId('register-harvest-row')).toHaveLength(2)
  })

  test('unticking a crop removes its card', async () => {
    renderScreen()
    await chooseCrop('Mahindi')
    await chooseCrop('Kahawa')
    await chooseCrop('Mahindi')

    const cards = screen.getAllByTestId('register-cycle-card')
    expect(cards).toHaveLength(1)
    expect(cards[0]).toHaveAttribute('data-crop-id', COFFEE)
  })

  test('both crops are sent, each with its own measure and harvest', async () => {
    renderScreen()
    await fillValid()
    await chooseCrop('Kahawa')
    const coffee = screen.getAllByTestId('register-cycle-card')[1]
    fireEvent.change(within(coffee).getByTestId('register-cycle-tree-count'), { target: { value: '40' } })
    const rows = screen.getAllByTestId('register-harvest-row')
    fireEvent.change(within(rows[0]).getByTestId('register-harvest-kg'), { target: { value: '2000' } })
    fireEvent.change(within(rows[1]).getByTestId('register-harvest-kg'), { target: { value: '300' } })
    submit()

    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1))
    const payload = rpc.mock.calls[0][1].payload as {
      cycles: Array<{ crop_id: string; area_ha: string; tree_count: string; harvest: { quantity_kg: string } }>
    }
    expect(payload.cycles).toHaveLength(2)
    expect(payload.cycles[0]).toMatchObject({ crop_id: MAIZE, area_ha: '1.6', harvest: { quantity_kg: '2000' } })
    expect(payload.cycles[1]).toMatchObject({ crop_id: COFFEE, tree_count: '40', harvest: { quantity_kg: '300' } })
  })

  test('says planted area across crops can exceed the plot area', () => {
    renderScreen()
    expect(screen.getByTestId('register-plot-area-note')).toHaveTextContent(/across/i)
  })

  // A phone mid-registration when the app updated: its one crop restores as
  // the first of the list, with everything typed for it.
  test('a draft from the one-crop form restores its crop', async () => {
    const { cycles: _cycles, ...rest } = EMPTY_DRAFT
    useDraft.mockReturnValue({
      status: 'dirty',
      draft: {
        ...rest,
        given_name: 'Neema',
        crop_id: MAIZE,
        cycle_area_ha: '1.4',
        cycle_tree_count: '',
        cycle_unit_count: '',
        harvest_start: '',
        harvest_end: '',
        harvest_quantity_kg: '900',
      },
      save,
      clear,
    })
    renderScreen()

    await waitFor(() => expect(screen.getByTestId('register-cycle-area')).toHaveValue('1.4'))
    expect(screen.getByRole('checkbox', { name: 'Mahindi' })).toBeChecked()
    expect(screen.getByTestId('register-harvest-kg')).toHaveValue('900')
  })
})

/**
 * Title documents, photographed at registration so whoever verifies the plot
 * can check it against them. Staff only. They are kept in their own draft
 * until the plot exists, then filed against it.
 */
describe('title documents', () => {
  const photo = (name: string) => new File(['abc'], name, { type: 'image/jpeg' })
  const pick = (...files: File[]) =>
    fireEvent.change(screen.getByTestId('register-plot-documents'), { target: { files } })

  test('picked files are listed and kept in their own draft', async () => {
    renderScreen()
    pick(photo('title.jpg'), photo('survey.jpg'))

    expect(await screen.findAllByTestId('register-document-item')).toHaveLength(2)
    await waitFor(() => expect(docsSave).toHaveBeenLastCalledWith(
      expect.arrayContaining([expect.objectContaining({ name: 'title.jpg' }), expect.objectContaining({ name: 'survey.jpg' })]),
    ))
  })

  test('a picked file can be taken off again', async () => {
    renderScreen()
    pick(photo('title.jpg'), photo('survey.jpg'))
    await userEvent.click((await screen.findAllByTestId('register-document-remove'))[0])

    expect(screen.getAllByTestId('register-document-item')).toHaveLength(1)
    expect(screen.getByTestId('register-document-item')).toHaveTextContent('survey.jpg')
  })

  test('says the farmer does not see them', () => {
    renderScreen()
    expect(screen.getByTestId('register-documents-note')).toHaveTextContent(/staff/i)
  })

  test('a reload brings the picked files back', async () => {
    docsDraft.mockReturnValue({
      status: 'dirty',
      draft: [{ id: 'f1', name: 'title.jpg', type: 'image/jpeg', size: 3, blob: new Blob(['abc']) }],
      save: docsSave,
      clear: docsClear,
    })
    renderScreen()

    expect(await screen.findByTestId('register-document-item')).toHaveTextContent('title.jpg')
  })

  test('once the farmer is registered, they are filed against the new plot', async () => {
    rpc.mockResolvedValue({ data: { person_id: 'new-person', plot_id: 'new-plot', village_id: VILLAGE }, error: null })
    uploadPlotDocuments.mockImplementation(async (_v, _p, files: Array<{ id: string }>) => ({
      filed: files.map((f) => f.id),
      failed: [],
    }))
    renderScreen()
    await fillValid()
    pick(photo('title.jpg'))
    submit()

    await screen.findByTestId('register-documents-filed')
    expect(uploadPlotDocuments).toHaveBeenCalledWith(VILLAGE, 'new-plot', [expect.objectContaining({ name: 'title.jpg' })])
    await waitFor(() => expect(docsClear).toHaveBeenCalled())
  })

  // The registration is saved either way. What was not filed says so, keeps
  // its draft, and can be tried again.
  test('a file that failed is named, kept and offered again', async () => {
    rpc.mockResolvedValue({ data: { person_id: 'new-person', plot_id: 'new-plot', village_id: VILLAGE }, error: null })
    uploadPlotDocuments.mockImplementationOnce(async (_v, _p, files: Array<{ id: string; name: string }>) => ({
      filed: [],
      failed: files.map((f) => ({ id: f.id, name: f.name, message: 'Payload too large' })),
    }))
    renderScreen()
    await fillValid()
    pick(photo('title.jpg'))
    submit()

    const failed = await screen.findByTestId('register-documents-failed')
    expect(failed).toHaveTextContent('title.jpg')
    expect(failed).toHaveTextContent('Payload too large')
    expect(docsClear).not.toHaveBeenCalled()

    uploadPlotDocuments.mockImplementationOnce(async (_v, _p, files: Array<{ id: string }>) => ({
      filed: files.map((f) => f.id),
      failed: [],
    }))
    await userEvent.click(screen.getByTestId('register-documents-retry'))
    await screen.findByTestId('register-documents-filed')
  })

  test('no files picked, nothing to file', async () => {
    rpc.mockResolvedValue({ data: { person_id: 'new-person', plot_id: 'new-plot', village_id: VILLAGE }, error: null })
    renderScreen()
    await fillValid()
    submit()

    await screen.findByTestId('register-success')
    expect(uploadPlotDocuments).not.toHaveBeenCalled()
    expect(screen.queryByTestId('register-documents-filed')).not.toBeInTheDocument()
  })
})

/**
 * QA #23. Measured directly at the time: `register-submit.disabled` was false
 * immediately after the first click and still false after three clicks in the
 * same tick. Spec 5.2 names six states for this screen and one of them is
 * SAVING — there was no saving state at all.
 *
 * `submit.isPending` alone cannot cover it: react-hook-form validates
 * asynchronously, so the mutation has not started yet on the tick the officer
 * clicks again.
 */
describe('the saving state spec 5.2 asks for', () => {
  test('three submits in the same tick register one farmer', async () => {
    // A write that never settles, which is what a slow rural connection looks
    // like from the officer's side.
    rpc.mockImplementation(() => new Promise(() => {}))
    renderScreen()
    await fillValid()

    submit()
    submit()
    submit()

    await waitFor(() => expect(rpc).toHaveBeenCalledTimes(1))
    expect(rpc).toHaveBeenCalledTimes(1)
  })

  test('and the control says what is happening rather than nothing', async () => {
    rpc.mockImplementation(() => new Promise(() => {}))
    renderScreen()
    await fillValid()
    submit()

    await waitFor(() => expect(screen.getByTestId('register-submit')).toBeDisabled())
    expect(screen.getByTestId('register-submit')).toHaveTextContent(/registering/i)
  })

  // A refused form is not a saving form: the officer has to be able to fix it
  // and submit again.
  test('a form that failed validation is submittable again', async () => {
    renderScreen()
    await fillValid()
    fireEvent.change(screen.getByTestId('register-given-name'), { target: { value: '   ' } })
    submit()

    await waitFor(() =>
      expect(screen.getByTestId('register-given-name-error')).toBeInTheDocument(),
    )
    expect(screen.getByTestId('register-submit')).toBeEnabled()
  })
})

/**
 * QA #22, at the screen. The corrupt draft restored verbatim, the badge said
 * "not yet submitted", and the form would have sent `[object Object]` as a
 * farmer's first name.
 */
describe('a draft that no longer matches this form', () => {
  /**
   * `useDraft` is what discards it — see useDraft.test.ts. What the SCREEN
   * owes is the other half: a discarded draft leaves an empty form and no
   * badge claiming there is unsaved work to come back to.
   */
  test('leaves an empty form and no unsaved badge', async () => {
    useDraft.mockReturnValue({ status: 'empty', draft: undefined, save, clear })
    renderScreen()

    await waitFor(() => expect(screen.getByTestId('register-given-name')).toHaveValue(''))
    expect(screen.getByTestId('register-family-name')).toHaveValue('')
    expect(screen.queryByTestId('unsaved-draft-badge')).not.toBeInTheDocument()
  })

  test('and the draft store is told to check the shape', () => {
    renderScreen()
    // The guard is passed to useDraft, which is what discards and clears it.
    const formDraft = [...useDraftArgs].reverse().find((args) => String(args[0]).startsWith('register:'))
    expect(formDraft?.[2]).toBe(isRegisterDraft)
  })

  // A good draft still restores: the point is the feature, not the check.
  test('a draft that does match is still restored', async () => {
    useDraft.mockReturnValue({
      status: 'dirty',
      draft: { ...EMPTY_DRAFT, given_name: 'Neema', family_name: 'Mwakalinga' },
      save,
      clear,
    })
    renderScreen()

    await waitFor(() => expect(screen.getByTestId('register-given-name')).toHaveValue('Neema'))
  })
})

/**
 * The unsaved-draft badge is required by business-rules.md §12 and asserted
 * by Playwright via its testid — it can move, but it must still exist, and
 * exactly once. Two copies (a header pill plus a footer echo) was the bug
 * being fixed here.
 */
describe('the unsaved-draft badge', () => {
  test('appears once, in the submit bar, when the draft is dirty', async () => {
    useDraft.mockReturnValue({
      status: 'dirty',
      draft: { ...EMPTY_DRAFT, given_name: 'Neema' },
      save,
      clear,
    })
    renderScreen()

    await waitFor(() => expect(screen.getByTestId('register-given-name')).toHaveValue('Neema'))
    expect(screen.getAllByTestId('unsaved-draft-badge')).toHaveLength(1)
    expect(
      screen.getByTestId('register-submit-bar').querySelector('[data-testid="unsaved-draft-badge"]'),
    ).not.toBeNull()
  })

  test('is absent when the draft is clean', async () => {
    useDraft.mockReturnValue({ status: 'clean', draft: null, save, clear })
    renderScreen()

    expect(screen.queryByTestId('unsaved-draft-badge')).not.toBeInTheDocument()
  })
})

/**
 * A section's card gets a soft green border once its own fields are filled —
 * the same completion the progress rail already computes, just reflected on
 * the card too (business-rules §22 territory: a hint, not a validation claim).
 */
describe('a completed section card', () => {
  test('is marked complete once its required fields are filled', async () => {
    renderScreen()

    const person = screen.getByRole('group', { name: /Person/ })
    expect(person).toHaveAttribute('data-complete', 'no')

    fireEvent.change(screen.getByTestId('register-given-name'), { target: { value: 'Neema' } })
    fireEvent.change(screen.getByTestId('register-family-name'), { target: { value: 'Mwakalinga' } })

    await waitFor(() => expect(person).toHaveAttribute('data-complete', 'yes'))
  })
})

/**
 * The Farm section's GPS auto-capture: `docs/screens-and-components.md`'s
 * `GpsCapture` states, wired into this screen's own coordinate fields rather
 * than a separate component.
 */
describe('farm GPS auto-capture', () => {
  afterEach(() => {
    Reflect.deleteProperty(navigator, 'geolocation')
  })

  function mockGeolocation() {
    const getCurrentPosition = vi.fn()
    Object.defineProperty(navigator, 'geolocation', {
      value: { getCurrentPosition },
      configurable: true,
    })
    return getCurrentPosition
  }

  test('an unsupported browser leaves the fields enabled, with an explanatory note and no locate or retry button', async () => {
    Reflect.deleteProperty(navigator, 'geolocation')
    renderScreen()

    await screen.findByTestId('register-gps-status')
    expect(screen.getByTestId('register-gps-status')).toHaveTextContent(
      /can't detect location/i,
    )
    expect(screen.queryByTestId('register-gps-retry')).not.toBeInTheDocument()
    expect(screen.queryByTestId('register-gps-locate')).not.toBeInTheDocument()
    expect(screen.getByTestId('register-farm-latitude')).toBeEnabled()
    expect(screen.getByTestId('register-farm-longitude')).toBeEnabled()
  })

  test('a successful read fills both coordinate fields and leaves them editable', async () => {
    const getCurrentPosition = mockGeolocation()
    renderScreen()

    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(1))
    const [success] = getCurrentPosition.mock.calls[0]
    act(() => success({ coords: { latitude: -7.1, longitude: 34.9 } }))

    await waitFor(() => expect(screen.getByTestId('register-farm-latitude')).toHaveValue('-7.100000'))
    expect(screen.getByTestId('register-farm-longitude')).toHaveValue('34.900000')
    expect(screen.getByTestId('register-farm-latitude')).toBeEnabled()
    expect(screen.getByTestId('register-farm-longitude')).toBeEnabled()
  })

  test('the fields are editable while the automatic read is still in flight, and typing wins', async () => {
    const getCurrentPosition = mockGeolocation()
    renderScreen()

    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(1))
    const latitude = screen.getByTestId('register-farm-latitude')
    expect(latitude).toBeEnabled()
    fireEvent.change(latitude, { target: { value: '-3.5' } })

    const [success] = getCurrentPosition.mock.calls[0]
    act(() => success({ coords: { latitude: -7.1, longitude: 34.9 } }))

    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(1))
    expect(screen.getByTestId('register-farm-latitude')).toHaveValue('-3.5')
    expect(screen.getByTestId('register-farm-longitude')).toHaveValue('')
  })

  test('"Use my current location" re-reads the GPS and overwrites both fields', async () => {
    const getCurrentPosition = mockGeolocation()
    renderScreen()

    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(1))
    act(() => getCurrentPosition.mock.calls[0][0]({ coords: { latitude: -7.1, longitude: 34.9 } }))
    await waitFor(() => expect(screen.getByTestId('register-farm-latitude')).toHaveValue('-7.100000'))

    fireEvent.change(screen.getByTestId('register-farm-latitude'), { target: { value: '5' } })
    await userEvent.click(screen.getByTestId('register-gps-locate'))
    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(2))
    act(() => getCurrentPosition.mock.calls[1][0]({ coords: { latitude: -8.2, longitude: 35.3 } }))

    await waitFor(() => expect(screen.getByTestId('register-farm-latitude')).toHaveValue('-8.200000'))
    expect(screen.getByTestId('register-farm-longitude')).toHaveValue('35.300000')
  })

  test('the locate button also works over a restored draft, where no automatic read ran', async () => {
    const getCurrentPosition = mockGeolocation()
    useDraft.mockReturnValue({
      status: 'dirty',
      draft: { ...EMPTY_DRAFT, farm_latitude: '-6.5', farm_longitude: '35.5' },
      save,
      clear,
    })
    renderScreen()

    await userEvent.click(await screen.findByTestId('register-gps-locate'))
    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(1))
    act(() => getCurrentPosition.mock.calls[0][0]({ coords: { latitude: 1.5, longitude: 2.5 } }))

    await waitFor(() => expect(screen.getByTestId('register-farm-latitude')).toHaveValue('1.500000'))
    expect(screen.getByTestId('register-farm-longitude')).toHaveValue('2.500000')
  })

  test('a denied read shows why and offers a retry, which can then succeed', async () => {
    const getCurrentPosition = mockGeolocation()
    renderScreen()

    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(1))
    const [, failure] = getCurrentPosition.mock.calls[0]
    act(() => failure({ code: 1 }))

    await screen.findByTestId('register-gps-retry')
    expect(screen.getByTestId('register-gps-status')).toHaveTextContent(/declined/i)
    // One way to try again, not two.
    expect(screen.queryByTestId('register-gps-locate')).not.toBeInTheDocument()
    expect(screen.getByTestId('register-farm-latitude')).toBeEnabled()

    await userEvent.click(screen.getByTestId('register-gps-retry'))
    await waitFor(() => expect(getCurrentPosition).toHaveBeenCalledTimes(2))
    const [secondSuccess] = getCurrentPosition.mock.calls[1]
    act(() => secondSuccess({ coords: { latitude: 1, longitude: 2 } }))

    await waitFor(() => expect(screen.getByTestId('register-farm-latitude')).toHaveValue('1.000000'))
    expect(screen.getByTestId('register-farm-latitude')).toBeEnabled()
  })

  // The reload-survives-a-draft guarantee: a value already in the draft is the
  // officer's, typed or previously captured, and a fresh sensor read must not
  // silently replace it.
  test('a restored draft with its own coordinates is never overwritten by a GPS read', async () => {
    const getCurrentPosition = mockGeolocation()
    useDraft.mockReturnValue({
      status: 'dirty',
      draft: { ...EMPTY_DRAFT, farm_latitude: '-6.5', farm_longitude: '35.5' },
      save,
      clear,
    })
    renderScreen()

    await waitFor(() => expect(screen.getByTestId('register-farm-latitude')).toHaveValue('-6.5'))
    expect(screen.getByTestId('register-farm-longitude')).toHaveValue('35.5')
    expect(screen.getByTestId('register-farm-latitude')).toBeEnabled()
    expect(getCurrentPosition).not.toHaveBeenCalled()
  })
})
