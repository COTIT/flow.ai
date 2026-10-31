import { fireEvent, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useEquipmentList = vi.fn()
const useLoanProducts = vi.fn()
const createEquipment = vi.fn()
const createLoan = vi.fn()
const createState = { isPending: false, isError: false, error: null as Error | null }
const search = { kind: undefined as string | undefined }
const navigate = vi.fn()

vi.mock('@/features/farmer/useEquipment', () => ({
  useEquipmentList: () => useEquipmentList(),
}))
vi.mock('@/features/farmer/useResources', () => ({
  useLoanProducts: () => useLoanProducts(),
}))
vi.mock('@/features/ops/useCatalogue', () => ({
  useEquipmentCategories: () => ({
    isLoading: false,
    data: [{ id: 'cat1', name: 'Milling' }, { id: 'cat2', name: 'Irrigation' }],
  }),
  useCreateEquipment: () => ({ mutateAsync: createEquipment, reset: vi.fn(), ...createState }),
  useCreateLoanProduct: () => ({ mutateAsync: createLoan, reset: vi.fn(), ...createState }),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => search, useNavigate: () => navigate }),
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

const { CatalogueScreen } = await import('@/features/ops/CatalogueScreen')
await import('@/i18n')

const item = (over: Record<string, unknown> = {}) => ({
  id: '51000000-0000-4000-8000-000000000001',
  name: 'Maize mill 500 kg/hr',
  category_name: 'Milling',
  rated_power_kw: 15,
  typical_hours_per_day: 6,
  typical_days_per_week: 5,
  indicative_price: 22000000,
  indicative_rent_per_day: 150000,
  can_rent: true,
  can_buy: true,
  currency: 'TZS',
  ...over,
})

const loan = (over: Record<string, unknown> = {}) => ({
  id: '52000000-0000-4000-8000-000000000001',
  code: 'INPUT',
  name: 'Input loan: seed and fertiliser',
  description: 'For a season of seed and fertiliser.',
  indicative_min_amount: 200000,
  indicative_max_amount: 1500000,
  currency: 'TZS',
  ...over,
})

beforeEach(() => {
  useEquipmentList.mockReset()
  useLoanProducts.mockReset()
  createEquipment.mockReset()
  createLoan.mockReset()
  navigate.mockReset()
  createEquipment.mockResolvedValue(undefined)
  createLoan.mockResolvedValue(undefined)
  createState.isPending = false
  createState.isError = false
  createState.error = null
  search.kind = undefined
  useEquipmentList.mockReturnValue({ isLoading: false, error: null, items: [item()] })
  useLoanProducts.mockReturnValue({ isLoading: false, error: null, items: [loan()] })
})

describe('CatalogueScreen states', () => {
  test('loading shows a loading state, not an empty message', () => {
    useEquipmentList.mockReturnValue({ isLoading: true, error: null, items: [] })
    render(<CatalogueScreen />)

    expect(screen.getByTestId('catalogue-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    useEquipmentList.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      items: [],
      refetch: vi.fn(),
    })
    render(<CatalogueScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByText('could not reach the database')).toBeInTheDocument()
  })

  test('an empty catalogue is an empty state, not an error', () => {
    useEquipmentList.mockReturnValue({ isLoading: false, error: null, items: [] })
    render(<CatalogueScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('the resource catalogue', () => {
  test('is a resource catalogue, not only equipment', () => {
    render(<CatalogueScreen />)

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent(/resource catalogue/i)
    expect(screen.getByTestId('catalogue-tab-equipment')).toHaveAttribute('aria-selected', 'true')
    expect(screen.getByTestId('catalogue-tab-loan')).toHaveAttribute('aria-selected', 'false')
  })

  test('switching kind is held in the URL', async () => {
    render(<CatalogueScreen />)
    await userEvent.click(screen.getByTestId('catalogue-tab-loan'))

    expect(navigate).toHaveBeenCalledWith(expect.objectContaining({ search: { kind: 'loan' } }))
  })

  test('lists equipment with the columns spec 7.4 names', () => {
    render(<CatalogueScreen />)

    const table = screen.getByTestId('catalogue-table')
    expect(table).toHaveTextContent('Maize mill 500 kg/hr')
    expect(table).toHaveTextContent('Milling')
    expect(table).toHaveTextContent('15.000 kW')
    expect(table).toHaveTextContent('TZS 22,000,000.00')
  })

  test('says how each machine is offered, with its rent per day', () => {
    useEquipmentList.mockReturnValue({
      isLoading: false,
      error: null,
      items: [item(), item({ id: 'x2', name: 'Cold room', can_rent: false, indicative_rent_per_day: null })],
    })
    render(<CatalogueScreen />)

    const [mill, cold] = screen.getAllByTestId('catalogue-offered')
    expect(mill).toHaveTextContent(/rent/i)
    expect(mill).toHaveTextContent(/buy/i)
    expect(cold).not.toHaveTextContent(/rent/i)
    expect(screen.getAllByTestId('catalogue-rent')[0]).toHaveTextContent('TZS 150,000.00')
    expect(screen.getAllByTestId('catalogue-rent')[0]).toHaveTextContent(/indicative/i)
  })

  // "Prices are always labelled indicative. They are not quotations."
  test('the price is labelled indicative', () => {
    render(<CatalogueScreen />)

    expect(screen.getByTestId('catalogue-price')).toHaveTextContent(/indicative/i)
    expect(screen.getByTestId('catalogue-note')).toHaveTextContent(/not quotations/i)
  })

  test('a null rated power is shown as unknown, not as zero', () => {
    useEquipmentList.mockReturnValue({
      isLoading: false,
      error: null,
      items: [item({ rated_power_kw: null, indicative_price: null })],
    })
    render(<CatalogueScreen />)

    const table = screen.getByTestId('catalogue-table')
    expect(table).not.toHaveTextContent('0.000 kW')
    expect(table).toHaveTextContent('—')
  })
})

/**
 * Loans are LISTINGS: a name, a description and an indicative range. No
 * finance terms — research item C is open — and never an offer.
 */
describe('loan listings', () => {
  beforeEach(() => {
    search.kind = 'loan'
  })

  test('show their indicative range and say they are not an offer', () => {
    render(<CatalogueScreen />)

    const row = screen.getByTestId('catalogue-loan-row')
    expect(row).toHaveTextContent('Input loan: seed and fertiliser')
    expect(screen.getByRole('columnheader', { name: 'Loan' })).toBeInTheDocument()
    expect(screen.getByTestId('catalogue-loan-range')).toHaveTextContent('TZS 200,000.00 – TZS 1,500,000.00')
    expect(screen.getByTestId('catalogue-loan-range')).toHaveTextContent(/indicative/i)
    expect(screen.getByTestId('catalogue-loan-note')).toHaveTextContent(/not an offer/i)
  })

  test('carry no finance terms', () => {
    render(<CatalogueScreen />)

    expect(screen.getByTestId('catalogue-loans-table')).not.toHaveTextContent(/interest|repay|deposit|instal/i)
  })

  test('none listed is an empty state', () => {
    useLoanProducts.mockReturnValue({ isLoading: false, error: null, items: [] })
    render(<CatalogueScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })
})

describe('adding to the catalogue', () => {
  const set = (testId: string, value: string) =>
    fireEvent.change(screen.getByTestId(testId), { target: { value } })
  const submit = () =>
    fireEvent.submit(screen.getByTestId('catalogue-create-submit').closest('form')!)

  test('stays collapsed until asked for', () => {
    render(<CatalogueScreen />)

    expect(screen.queryByTestId('catalogue-create-panel')).not.toBeInTheDocument()
    expect(screen.getByTestId('catalogue-create-open')).toHaveAttribute('aria-expanded', 'false')
  })

  test('adds a machine offered for rent, with its rent per day', async () => {
    render(<CatalogueScreen />)
    await userEvent.click(screen.getByTestId('catalogue-create-open'))

    set('catalogue-code', 'HAMMER')
    set('catalogue-name-en', 'Hammer mill')
    set('catalogue-name-sw', 'Mashine ya nyundo')
    await userEvent.click(screen.getByTestId('catalogue-category'))
    await userEvent.click(await screen.findByRole('option', { name: 'Milling' }))
    set('catalogue-power', '5.5')
    await userEvent.click(screen.getByTestId('catalogue-can-rent'))
    await userEvent.click(screen.getByTestId('catalogue-can-buy'))
    set('catalogue-rent-price', '60000')
    submit()

    expect(createEquipment).toHaveBeenCalledTimes(1)
    expect(createEquipment.mock.calls[0][0]).toMatchObject({
      project_id: '20000000-0000-4000-8000-000000000001',
      category_id: 'cat1',
      code: 'HAMMER',
      name_en: 'Hammer mill',
      name_sw: 'Mashine ya nyundo',
      rated_power_kw: 5.5,
      can_rent: true,
      can_buy: false,
      indicative_rent_per_day: 60000,
      indicative_price: null,
    })
  })

  // Reference data is translated in the database: both names are required.
  test('a machine needs a code, both names and a category', async () => {
    render(<CatalogueScreen />)
    await userEvent.click(screen.getByTestId('catalogue-create-open'))
    submit()

    for (const id of ['catalogue-code-error', 'catalogue-name-en-error', 'catalogue-name-sw-error', 'catalogue-category-error']) {
      expect(screen.getByTestId(id)).toBeInTheDocument()
    }
    expect(createEquipment).not.toHaveBeenCalled()
  })

  test('a price that is not a number is refused, not dropped', async () => {
    render(<CatalogueScreen />)
    await userEvent.click(screen.getByTestId('catalogue-create-open'))
    set('catalogue-buy-price', '12,5')
    submit()

    expect(screen.getByTestId('catalogue-buy-price-error')).toBeInTheDocument()
    expect(createEquipment).not.toHaveBeenCalled()
  })

  test('adds a loan listing with its indicative range, and nothing else', async () => {
    search.kind = 'loan'
    render(<CatalogueScreen />)
    await userEvent.click(screen.getByTestId('catalogue-create-open'))

    expect(within(screen.getByTestId('catalogue-create-panel')).queryByText(/interest|repay|deposit|term/i)).toBeNull()
    set('catalogue-code', 'WOMEN')
    set('catalogue-name-en', 'Women group loan')
    set('catalogue-name-sw', 'Mkopo wa kikundi cha wanawake')
    set('catalogue-min', '500000')
    set('catalogue-max', '3000000')
    submit()

    expect(createLoan).toHaveBeenCalledTimes(1)
    expect(createLoan.mock.calls[0][0]).toMatchObject({
      project_id: '20000000-0000-4000-8000-000000000001',
      code: 'WOMEN',
      name_en: 'Women group loan',
      name_sw: 'Mkopo wa kikundi cha wanawake',
      indicative_min_amount: 500000,
      indicative_max_amount: 3000000,
      description_en: null,
      description_sw: null,
    })
    expect(createEquipment).not.toHaveBeenCalled()
  })

  test("the database's refusal is shown", async () => {
    createState.isError = true
    createState.error = new Error('new row for relation "equipment" violates check constraint "equipment_offered"')
    render(<CatalogueScreen />)
    await userEvent.click(screen.getByTestId('catalogue-create-open'))

    expect(screen.getByTestId('catalogue-create-error')).toHaveTextContent(/rent, to buy, or both/i)
  })
})
