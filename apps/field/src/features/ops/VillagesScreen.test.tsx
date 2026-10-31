import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useVillageCapacity = vi.fn()
const mutateAsync = vi.fn()
const createState = { isPending: false, isError: false, error: null as Error | null }
vi.mock('@/features/ops/useOpsReference', () => ({
  useVillageCapacity: () => useVillageCapacity(),
  useCreateVillage: () => ({ mutateAsync, reset: vi.fn(), ...createState }),
}))
vi.mock('@/app/session', () => ({
  useSession: () => ({
    data: {
      userId: undefined,
      memberships: [
        { id: 'm1', role: 'ops', project_id: '20000000-0000-4000-8000-000000000001', village_id: null, revoked_at: null },
      ],
    },
  }),
}))
vi.mock('@/lib/drafts', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/drafts')>()
  return { ...actual, indexedDbDraftStore: actual.createMemoryDraftStore() }
})

const { VillagesScreen } = await import('@/features/ops/VillagesScreen')
await import('@/i18n')

beforeEach(() => {
  useVillageCapacity.mockReset()
  mutateAsync.mockReset()
  mutateAsync.mockResolvedValue({ village_id: 'v1', replayed: false })
  createState.isPending = false
  createState.isError = false
  createState.error = null
})

const village = (over: Record<string, unknown> = {}) => ({
  id: '30000000-0000-4000-8000-000000000001',
  name: 'Ilundo',
  code: 'ILU',
  capacity_kw: 500,
  basis: 'planned',
  simultaneity_factor: 0.6,
  effective_from: '2026-01-01',
  source_note: null,
  ...over,
})

/**
 * Spec 7.9 — villages with `village_capacity`, `basis` and
 * `simultaneity_factor` shown explicitly.
 *
 * The labelling here is a product requirement, not copy preference: capacity
 * is PLANNED, never measured, and `capacity_basis` has no 'measured' value on
 * purpose.
 */
describe('VillagesScreen states', () => {
  test('loading shows a loading state', () => {
    useVillageCapacity.mockReturnValue({ isLoading: true, error: null, data: [] })
    render(<VillagesScreen />)

    expect(screen.getByTestId('villages-loading')).toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    useVillageCapacity.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: [],
      refetch: vi.fn(),
    })
    render(<VillagesScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })

  test('no villages is an empty state, not an error', () => {
    useVillageCapacity.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<VillagesScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('VillagesScreen content', () => {
  test('shows capacity, its basis and the simultaneity factor', () => {
    useVillageCapacity.mockReturnValue({ isLoading: false, error: null, data: [village()] })
    render(<VillagesScreen />)

    const table = screen.getByTestId('villages-table')
    expect(table).toHaveTextContent('Ilundo')
    expect(table).toHaveTextContent('500.000 kW')
    expect(screen.getByTestId('village-basis')).toHaveTextContent('Planned')
    expect(table).toHaveTextContent('0.6')
  })

  test('states that capacity is planned and never measured', () => {
    useVillageCapacity.mockReturnValue({ isLoading: false, error: null, data: [village()] })
    render(<VillagesScreen />)

    expect(screen.getByTestId('villages-note')).toHaveTextContent(/planned, never measured/i)
  })

  // Village peak is not the sum of rated power, so the factor is explained
  // rather than left as a bare number.
  test('explains what the simultaneity factor is for', () => {
    useVillageCapacity.mockReturnValue({ isLoading: false, error: null, data: [village()] })
    render(<VillagesScreen />)

    expect(screen.getByText(/not the sum of rated power/i)).toBeInTheDocument()
  })

  // v_village_energy inner-joins capacity, so a village without a current row
  // has no energy figures at all. Absent must not read as zero.
  test('a village with no capacity row shows absent figures, not zeroes', () => {
    useVillageCapacity.mockReturnValue({
      isLoading: false,
      error: null,
      data: [
        village({
          name: 'Mgama',
          capacity_kw: null,
          basis: null,
          simultaneity_factor: null,
          effective_from: null,
        }),
      ],
    })
    render(<VillagesScreen />)

    const table = screen.getByTestId('villages-table')
    expect(table).toHaveTextContent('Mgama')
    expect(table).not.toHaveTextContent('0.000 kW')
    expect(table).toHaveTextContent('—')
  })

  test('never labels a planned figure as measured', () => {
    useVillageCapacity.mockReturnValue({ isLoading: false, error: null, data: [village()] })
    render(<VillagesScreen />)

    expect(screen.queryByText(/measured capacity/i)).not.toBeInTheDocument()
  })
})

/**
 * Ops adds a village with its first PLANNED capacity row. One call writes
 * both, because a village with no capacity row has no energy figures and
 * drops off the Tower.
 */
describe('the village create form', () => {
  const ready = () =>
    useVillageCapacity.mockReturnValue({ isLoading: false, error: null, data: [village()] })
  const set = (testId: string, value: string) =>
    fireEvent.change(screen.getByTestId(testId), { target: { value } })
  const submit = () =>
    fireEvent.submit(screen.getByTestId('village-create-submit').closest('form')!)
  const open = () => userEvent.click(screen.getByTestId('village-create-open'))
  const fill = async () => {
    await open()
    set('village-name', 'Mlowa')
    set('village-code', 'MLW')
    set('village-capacity', '250')
  }

  test('stays collapsed until asked for', () => {
    ready()
    render(<VillagesScreen />)

    expect(screen.queryByTestId('village-create-panel')).not.toBeInTheDocument()
    expect(screen.getByTestId('village-create-open')).toHaveAttribute('aria-expanded', 'false')
  })

  test('sends the village and its first planned capacity together', async () => {
    ready()
    render(<VillagesScreen />)
    await fill()
    set('village-latitude', '-7.91')
    set('village-longitude', '35.62')
    set('village-simultaneity', '0.65')

    submit()

    expect(mutateAsync).toHaveBeenCalledTimes(1)
    expect(mutateAsync.mock.calls[0][0]).toMatchObject({
      project_id: '20000000-0000-4000-8000-000000000001',
      name: 'Mlowa',
      code: 'MLW',
      latitude: '-7.91',
      longitude: '35.62',
      capacity: { capacity_kw: '250', basis: 'planned', simultaneity_factor: '0.65' },
    })
    expect(typeof mutateAsync.mock.calls[0][0].id).toBe('string')
  })

  test('labels the capacity as planned, with its basis', async () => {
    ready()
    render(<VillagesScreen />)
    await open()

    expect(screen.getByText(/planned capacity \(kW\)/i)).toBeInTheDocument()
    expect(screen.getByTestId('village-basis-select')).toHaveTextContent('Planned')
  })

  test('an empty form reports what is missing and writes nothing', async () => {
    ready()
    render(<VillagesScreen />)
    await open()

    submit()

    expect(screen.getByTestId('village-name-error')).toBeInTheDocument()
    expect(screen.getByTestId('village-code-error')).toBeInTheDocument()
    expect(screen.getByTestId('village-capacity-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a capacity that is not a number is refused, not dropped', async () => {
    ready()
    render(<VillagesScreen />)
    await fill()
    set('village-capacity', '25,5')

    submit()

    expect(screen.getByTestId('village-capacity-error')).toBeInTheDocument()
    expect(mutateAsync).not.toHaveBeenCalled()
  })

  test('a second submit in the same tick does not write twice', async () => {
    ready()
    render(<VillagesScreen />)
    await fill()

    submit()
    submit()

    expect(mutateAsync).toHaveBeenCalledTimes(1)
  })

  test("the database's refusal names the collision", async () => {
    ready()
    createState.isError = true
    createState.error = new Error('duplicate key value violates unique constraint "village_project_id_code_key"')
    render(<VillagesScreen />)
    await open()

    // The constraint name is machine noise; the app names the collision.
    expect(screen.getByTestId('village-create-error')).toHaveTextContent(/village with that code/i)
  })
})
