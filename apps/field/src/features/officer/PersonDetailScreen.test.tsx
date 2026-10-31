import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const usePersonDetail = vi.fn()
const useVerify = vi.fn()
const useCrops = vi.fn()
const mutate = vi.fn()
const reset = vi.fn()
const editMutate = vi.fn()
const editReset = vi.fn()
const queryState: { data: unknown; error: Error | null; isLoading: boolean; refetch: ReturnType<typeof vi.fn> } = {
  data: null,
  error: null,
  isLoading: false,
  refetch: vi.fn(),
}
const verifyState = {
  isPending: false,
  variables: undefined as { table: string; id: string } | undefined,
  isError: false,
  error: null as Error | null,
}

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ personId: 'p1' }) }),
}))
const sessionState: { data: { userId: string } | undefined } = { data: { userId: 'officer-1' } }
vi.mock('@/app/session', () => ({ useSession: () => sessionState }))
type HistoryRow = { issued_at: string; kind: string; issued_by_name: string; must_change_password: boolean }
const historyState: { data: HistoryRow[] | undefined; error: Error | null; refetch: ReturnType<typeof vi.fn> } = {
  data: [],
  error: null,
  refetch: vi.fn(),
}
vi.mock('@/features/officer/useFarmerLogin', () => ({ useLoginHistory: () => historyState }))
// The card has its own tests; here it is a stand-in that shows what it was given.
vi.mock('@/features/officer/FarmerLoginCard', () => ({
  FarmerLoginCard: ({ personId, hasLogin }: { personId: string; hasLogin?: boolean }) => (
    <div data-testid="farmer-login-card-stub" data-person={personId} data-has-login={String(Boolean(hasLogin))} />
  ),
}))
vi.mock('@/features/officer/usePersonDetail', () => ({
  usePersonDetail: () => usePersonDetail(),
  useVerify: () => useVerify(),
}))
vi.mock('@/features/officer/useCrops', () => ({ useCrops: () => useCrops() }))
vi.mock('@/features/officer/useOfficerEdit', () => ({
  useOfficerEdit: () => ({ mutateAsync: editMutate, reset: editReset, isPending: false, error: null }),
}))
vi.mock('@/features/officer/VerifyButton', () => ({
  VerifyButton: ({ table, id, onVerify }: { table: string; id: string; onVerify: (value: { table: string; id: string }) => void }) => (
    <button type="button" data-testid={`verify-${table}-${id}`} onClick={() => onVerify({ table, id })}>Verify</button>
  ),
}))
vi.mock('@/features/officer/OfficerEditDialog', () => ({
  OfficerEditDialog: ({ table, version, initialValues, onSave, onComplete, onCancel }: { table: string; version?: string | null; initialValues: Record<string, string>; onSave: (values: Record<string, string>) => Promise<unknown> | void; onComplete?: () => void; onCancel: () => void }) => (
    <div data-testid={`screen-edit-${table}`} data-version={version ?? ''}>
      <button type="button" data-testid="screen-edit-save" onClick={async () => { try { await onSave(initialValues); onComplete?.() } catch { /* stays open */ } }}>Save</button>
      <button type="button" data-testid="screen-edit-cancel" onClick={onCancel}>Cancel</button>
    </div>
  ),
}))

// The documents panel has its own tests; here only that each plot carries one.
vi.mock('@/features/officer/PlotDocuments', () => ({
  PlotDocuments: ({ plotId, villageId }: { plotId: string; villageId: string }) => (
    <div data-testid={`stub-plot-documents-${plotId}`} data-village={villageId} />
  ),
}))

const { PersonDetailScreen } = await import('@/features/officer/PersonDetailScreen')
await import('@/i18n')

const provenance = (verification: 'unverified' | 'pending' | 'verified' | 'disputed' = 'verified') => ({
  source: 'field_verified' as const,
  verification,
  confidence: null,
  captured_at: '2026-09-20T00:00:00Z',
  captured_by: null,
  verified_by: null,
  verified_at: null,
})

function detail(over: Record<string, unknown> = {}) {
  const areaCycle = {
    id: 'c-area', crop_id: 'crop-area', crop_name: 'Maize', season_label: 'Season', area_ha: 1, tree_count: null, unit_count: null,
    planted_on: '2026-01-01', harvest_start: '2026-08-01', harvest_end: '2026-09-01', status: 'growing' as const,
    ...provenance('pending'), harvests: [
      { id: 'h-current', kind: 'expected' as const, quantity_kg: 100, is_current: true, reported_for: '2026-08-02', ...provenance('unverified') },
      { id: 'h-old', kind: 'expected' as const, quantity_kg: 90, is_current: false, reported_for: null, ...provenance('verified') },
    ],
  }
  const treeCycle = {
    id: 'c-tree', crop_id: 'crop-tree', crop_name: 'Avocado', season_label: null, area_ha: null, tree_count: 12, unit_count: null,
    planted_on: null, harvest_start: null, harvest_end: null, status: 'planned' as const,
    ...provenance('verified'), harvests: [
      { id: 'h-tree-current', kind: 'actual' as const, quantity_kg: 8, is_current: true, reported_for: null, ...provenance('pending') },
    ],
  }
  const unitCycle = {
    id: 'c-unit', crop_id: 'crop-unit', crop_name: 'Eggs', season_label: 'Units', area_ha: null, tree_count: null, unit_count: 5,
    planted_on: null, harvest_start: null, harvest_end: null, status: 'harvested' as const,
    ...provenance('verified'), harvests: [],
  }
  return {
    person: { id: 'p1', given_name: 'Neema', family_name: 'Mwakalinga', phone: '0700', village_id: 'v1', ...provenance('unverified') },
    households: [{ id: 'hhold', label: 'Household', members: [{ id: 'p1', given_name: 'Neema', family_name: 'Mwakalinga' }], ...provenance('verified') }],
    farms: [{
      id: 'farm1', label: 'Farm', latitude: -8, longitude: 35, ...provenance('verified'),
      plots: [{ id: 'plot1', label: 'Plot', area_ha: 1.2, latitude: null, longitude: null, ...provenance('pending'), cycles: [areaCycle, treeCycle, unitCycle] }],
    }],
    ...over,
  }
}

beforeEach(() => {
  queryState.data = detail()
  queryState.error = null
  queryState.isLoading = false
  verifyState.isPending = false
  verifyState.variables = undefined
  verifyState.isError = false
  verifyState.error = null
  usePersonDetail.mockReturnValue(queryState)
  useVerify.mockReturnValue({ ...verifyState, mutate, reset })
  useCrops.mockReturnValue({ crops: [
    { id: 'crop-area', name: 'Maize', measured_by: 'area' },
    { id: 'crop-tree', name: 'Avocado', measured_by: 'tree_count' },
    { id: 'crop-unit', name: 'Eggs', measured_by: 'unit_count' },
  ] })
  mutate.mockReset()
  reset.mockReset()
  editMutate.mockReset()
  editReset.mockReset()
  sessionState.data = { userId: 'officer-1' }
  historyState.data = []
  historyState.error = null
  historyState.refetch = vi.fn()
})

describe('PersonDetailScreen', () => {
  test('renders the full graph and every section edit action', async () => {
    const user = userEvent.setup()
    verifyState.isPending = true
    verifyState.variables = { table: 'person', id: 'p1' }
    useVerify.mockReturnValue({ ...verifyState, mutate, reset })
    render(<PersonDetailScreen />)
    expect(screen.getByTestId('person-detail')).toHaveTextContent('Neema Mwakalinga')
    expect(screen.getByTestId('household-hhold')).toHaveTextContent('Household')
    expect(screen.getByTestId('farm-farm1')).toHaveTextContent('Farm')
    expect(screen.getByTestId('harvest-h-old')).toHaveTextContent('Superseded')

    for (const id of ['edit-person', 'edit-household-hhold', 'edit-farm-farm1', 'edit-plot-plot1', 'edit-cycle-c-area', 'edit-cycle-c-tree', 'edit-cycle-c-unit', 'edit-harvest-h-current', 'edit-harvest-h-tree-current']) {
      await user.click(screen.getByTestId(id))
      expect(screen.getByTestId(/screen-edit-(person|household|farm|plot|crop_cycle|harvest_report)$/)).toBeInTheDocument()
      await user.click(screen.getByTestId('screen-edit-cancel'))
    }
    expect(editReset).toHaveBeenCalled()
  })

  // The verifier checks a plot against its title documents, so they sit on it.
  test('each plot carries its title documents', () => {
    render(<PersonDetailScreen />)
    expect(within(screen.getByTestId('plot-plot1')).getByTestId('stub-plot-documents-plot1')).toBeInTheDocument()
  })

  // A season the officer can edit must also be visible where they edit it.
  test('a cycle shows its season beside the crop', () => {
    render(<PersonDetailScreen />)
    expect(screen.getByTestId('cycle-c-area')).toHaveTextContent('Maize · Season')
    expect(screen.getByTestId('cycle-c-tree')).toHaveTextContent('Avocado')
    expect(screen.getByTestId('cycle-c-tree')).not.toHaveTextContent('Avocado ·')
  })

  test('saves a section through the mutation and closes after success', async () => {
    const user = userEvent.setup()
    render(<PersonDetailScreen />)
    await user.click(screen.getByTestId('edit-person'))
    await user.click(screen.getByTestId('screen-edit-save'))
    expect(editMutate).toHaveBeenCalledTimes(1)
    await waitFor(() => expect(screen.queryByTestId('screen-edit-person')).not.toBeInTheDocument())
  })

  // The payload drops measures the crop does not use, so the save must name
  // the measure of the crop currently selected in the dialog.
  test('a cycle save passes the selected crop measure', async () => {
    const user = userEvent.setup()
    render(<PersonDetailScreen />)
    await user.click(screen.getByTestId('edit-cycle-c-tree'))
    await user.click(screen.getByTestId('screen-edit-save'))
    expect(editMutate).toHaveBeenCalledWith(expect.objectContaining({ table: 'crop_cycle', id: 'c-tree', measure: 'tree_count' }))
  })

  // A draft is discarded when the record changed since it was written, so
  // the dialog must be told which server version it is editing.
  test('each edit dialog is given the record captured_at as its draft version', async () => {
    const user = userEvent.setup()
    render(<PersonDetailScreen />)
    await user.click(screen.getByTestId('edit-person'))
    expect(screen.getByTestId('screen-edit-person')).toHaveAttribute('data-version', '2026-09-20T00:00:00Z')
  })

  test('a failed section save stays open', async () => {
    const user = userEvent.setup()
    editMutate.mockRejectedValueOnce(new Error('refused'))
    render(<PersonDetailScreen />)
    await user.click(screen.getByTestId('edit-person'))
    await user.click(screen.getByTestId('screen-edit-save'))
    expect(screen.getByTestId('screen-edit-person')).toBeInTheDocument()
  })

  test('handles loading, not-found, read error, and verify error states', async () => {
    queryState.isLoading = true
    const view = render(<PersonDetailScreen />)
    expect(screen.getByTestId('person-loading')).toBeInTheDocument()

    queryState.isLoading = false
    queryState.data = null
    view.rerender(<PersonDetailScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()

    queryState.data = detail()
    queryState.error = new Error('read failed')
    view.rerender(<PersonDetailScreen />)
    expect(screen.getByTestId('error-state')).toHaveTextContent('read failed')
    await userEvent.setup().click(screen.getByTestId('error-retry'))
    expect(queryState.refetch).toHaveBeenCalled()

    queryState.error = null
    verifyState.isError = true
    verifyState.error = new Error('verify failed')
    useVerify.mockReturnValue({ ...verifyState, mutate, reset })
    view.rerender(<PersonDetailScreen />)
    expect(screen.getByTestId('verify-error')).toHaveTextContent('verify failed')
    await userEvent.setup().click(screen.getByTestId('error-retry'))
    expect(reset).toHaveBeenCalled()
  })

  test('renders empty relationship sections and fully verified summary', () => {
    queryState.data = {
      ...detail({ households: [], farms: [] }),
      person: { ...detail().person, phone: null, verification: 'verified' },
    }
    render(<PersonDetailScreen />)
    expect(screen.getByTestId('person-outstanding')).toHaveTextContent('Every record here is verified')
    expect(screen.getAllByTestId('empty-state')).toHaveLength(2)
  })

  test('preloads nullable correction fields', async () => {
    const user = userEvent.setup()
    const base = detail()
    queryState.data = {
      ...base,
      farms: [{
        ...base.farms[0],
        latitude: null,
        longitude: null,
        plots: [{ ...base.farms[0].plots[0], area_ha: null, latitude: 1, longitude: 2, cycles: [] }],
      }],
      person: { ...base.person, phone: null },
    }
    render(<PersonDetailScreen />)
    await user.click(screen.getByTestId('edit-person'))
    await user.click(screen.getByTestId('screen-edit-cancel'))
    await user.click(screen.getByTestId('edit-farm-farm1'))
    await user.click(screen.getByTestId('screen-edit-cancel'))
    await user.click(screen.getByTestId('edit-plot-plot1'))
    await user.click(screen.getByTestId('screen-edit-cancel'))
  })
})

/**
 * Four eyes on households (20260929090002_household_four_eyes). The officer
 * who registered a household may not verify it: app_verify refuses, and the
 * screen says so instead of offering a button that can only fail.
 */
describe('PersonDetailScreen household four eyes', () => {
  const withHousehold = (over: Record<string, unknown>) => {
    const base = detail()
    return { ...base, households: [{ ...base.households[0], ...over }] }
  }

  test('a household you registered shows the note, not Verify; the other records keep theirs', () => {
    queryState.data = withHousehold({ verification: 'unverified', captured_by: 'officer-1' })
    render(<PersonDetailScreen />)

    expect(screen.queryByTestId('verify-household-hhold')).not.toBeInTheDocument()
    expect(screen.getByTestId('verify-needs-second-staff-hhold')).toBeInTheDocument()
    expect(screen.getByTestId('verify-person-p1')).toBeInTheDocument()
    expect(screen.getByTestId('verify-farm-farm1')).toBeInTheDocument()
    expect(screen.getByTestId('verify-plot-plot1')).toBeInTheDocument()
  })

  // The count is what the records say. The household still needs verifying —
  // by somebody else — and hiding it would tell this officer the work is done.
  test('the outstanding count still includes that household', () => {
    const base = withHousehold({ verification: 'unverified', captured_by: 'officer-1' })
    queryState.data = { ...base, person: { ...base.person, verification: 'verified' }, farms: [] }
    render(<PersonDetailScreen />)

    expect(screen.getByTestId('person-outstanding')).toHaveTextContent('1 record still needs verifying')
  })

  test("another officer's household keeps its Verify button", () => {
    queryState.data = withHousehold({ verification: 'pending', captured_by: 'officer-2' })
    render(<PersonDetailScreen />)

    expect(screen.getByTestId('verify-household-hhold')).toBeInTheDocument()
    expect(screen.queryByTestId('verify-needs-second-staff-hhold')).not.toBeInTheDocument()
  })

  test('a verified household you registered shows no note: there is nothing left to do', () => {
    queryState.data = withHousehold({ verification: 'verified', captured_by: 'officer-1' })
    render(<PersonDetailScreen />)

    expect(screen.queryByTestId('verify-needs-second-staff-hhold')).not.toBeInTheDocument()
  })

  // Convenience, not security: with no session read yet the button is offered
  // and the database is the one that refuses.
  test('before the session is known, Verify is offered and the database decides', () => {
    sessionState.data = undefined
    queryState.data = withHousehold({ verification: 'unverified', captured_by: 'officer-1' })
    render(<PersonDetailScreen />)

    expect(screen.getByTestId('verify-household-hhold')).toBeInTheDocument()
  })
})

describe('PersonDetailScreen app login', () => {
  test('no history: says so, and offers to create a login for this person', () => {
    render(<PersonDetailScreen />)

    expect(screen.getByRole('heading', { level: 2, name: 'App login' })).toBeInTheDocument()
    expect(screen.getByTestId('login-history-none')).toHaveTextContent('No app login yet.')
    expect(screen.getByTestId('farmer-login-card-stub')).toHaveAttribute('data-person', 'p1')
    expect(screen.getByTestId('farmer-login-card-stub')).toHaveAttribute('data-has-login', 'false')
    expect(screen.queryByTestId('login-must-change')).not.toBeInTheDocument()
  })

  test('history lists who created and who reset the login, newest first, and the action becomes a reset', () => {
    historyState.data = [
      { issued_at: '2026-09-29T09:00:00Z', kind: 'reset', issued_by_name: 'Juma Officer', must_change_password: false },
      { issued_at: '2026-09-20T09:00:00Z', kind: 'initial', issued_by_name: 'Asha Officer', must_change_password: false },
    ]
    render(<PersonDetailScreen />)

    const rows = screen.getAllByTestId('login-history-row')
    expect(rows).toHaveLength(2)
    expect(rows[0]).toHaveTextContent('Password reset by Juma Officer')
    expect(rows[0]).toHaveTextContent('29 Sep 2026, 12:00')
    expect(rows[1]).toHaveTextContent('Login created by Asha Officer')
    expect(screen.getByTestId('farmer-login-card-stub')).toHaveAttribute('data-has-login', 'true')
    expect(screen.queryByTestId('login-history-none')).not.toBeInTheDocument()
    expect(screen.queryByTestId('login-must-change')).not.toBeInTheDocument()
  })

  test('the newest row decides whether the farmer still has to choose a password', () => {
    historyState.data = [
      { issued_at: '2026-09-29T09:00:00Z', kind: 'initial', issued_by_name: 'Juma Officer', must_change_password: true },
    ]
    render(<PersonDetailScreen />)

    expect(screen.getByTestId('login-must-change')).toHaveTextContent(
      'Waiting for the farmer to choose their own password.',
    )
  })

  test('loading the history shows loading, and offers no action yet', () => {
    historyState.data = undefined
    render(<PersonDetailScreen />)

    expect(screen.getByTestId('login-history-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('farmer-login-card-stub')).not.toBeInTheDocument()
  })

  // A failed read is not "no login": offering Create there would reset the
  // password of a farmer who already has one.
  test('a failed history read is an error with retry, and offers no action', async () => {
    historyState.data = undefined
    historyState.error = new Error('history failed')
    render(<PersonDetailScreen />)

    const section = screen.getByTestId('app-login')
    expect(section).toHaveTextContent('history failed')
    expect(screen.queryByTestId('farmer-login-card-stub')).not.toBeInTheDocument()
    await userEvent.setup().click(within(section).getByTestId('error-retry'))
    expect(historyState.refetch).toHaveBeenCalled()
  })
})
