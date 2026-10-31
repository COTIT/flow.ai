import { render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useEquipmentList = vi.fn()
const useLoanProducts = vi.fn()
const search = { kind: undefined as string | undefined }
const navigate = vi.fn()

vi.mock('@/features/farmer/useEquipment', () => ({ useEquipmentList: () => useEquipmentList() }))
vi.mock('@/features/farmer/useResources', () => ({ useLoanProducts: () => useLoanProducts() }))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => search, useNavigate: () => navigate }),
  Link: ({ children, ...props }: { children: React.ReactNode }) => <a href="#x" {...props}>{children}</a>,
}))

const { EquipmentListScreen } = await import('@/features/farmer/EquipmentListScreen')
await import('@/i18n')

const machine = (over: Record<string, unknown> = {}) => ({
  id: 'eq1',
  name: 'Maize mill',
  category_name: 'Milling',
  rated_power_kw: 15,
  indicative_price: 22000000,
  indicative_rent_per_day: 150000,
  can_rent: true,
  can_buy: true,
  currency: 'TZS',
  ...over,
})

const loan = {
  id: 'l1',
  code: 'INPUT',
  name: 'Input loan',
  description: 'Seed and fertiliser for a season.',
  indicative_min_amount: 200000,
  indicative_max_amount: 1500000,
  currency: 'TZS',
}

beforeEach(() => {
  search.kind = undefined
  navigate.mockReset()
  useEquipmentList.mockReturnValue({ isLoading: false, error: null, items: [machine()] })
  useLoanProducts.mockReturnValue({ isLoading: false, error: null, items: [loan] })
})

describe('the Resources screen', () => {
  test('offers equipment and loans', () => {
    render(<EquipmentListScreen />)

    expect(screen.getByRole('heading', { level: 1 })).toHaveTextContent('Equipment and loans')
    expect(screen.getByTestId('resources-tab-equipment')).toHaveAttribute('aria-selected', 'true')
  })

  test('switching to loans is held in the URL', async () => {
    render(<EquipmentListScreen />)
    await userEvent.click(screen.getByTestId('resources-tab-loan'))

    expect(navigate).toHaveBeenCalledWith(expect.objectContaining({ search: { kind: 'loan' } }))
  })

  test('each machine says whether it can be rented or bought, at indicative prices', () => {
    render(<EquipmentListScreen />)

    const card = screen.getByTestId('equipment-item')
    expect(within(card).getByTestId('equipment-offered')).toHaveTextContent(/rent/i)
    expect(within(card).getByTestId('equipment-offered')).toHaveTextContent(/buy/i)
    expect(within(card).getByTestId('equipment-price')).toHaveTextContent('TZS 22,000,000.00')
    expect(within(card).getByTestId('equipment-price')).toHaveTextContent(/indicative/i)
    expect(within(card).getByTestId('equipment-rent')).toHaveTextContent('TZS 150,000.00')
    expect(within(card).getByTestId('equipment-rent')).toHaveTextContent(/indicative/i)
  })

  test('a machine only for sale shows no rent', () => {
    useEquipmentList.mockReturnValue({
      isLoading: false, error: null, items: [machine({ can_rent: false, indicative_rent_per_day: null })],
    })
    render(<EquipmentListScreen />)

    expect(screen.queryByTestId('equipment-rent')).not.toBeInTheDocument()
    expect(screen.getByTestId('equipment-offered')).not.toHaveTextContent(/rent/i)
  })

  test('no equipment is an empty state', () => {
    useEquipmentList.mockReturnValue({ isLoading: false, error: null, items: [] })
    render(<EquipmentListScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })
})

/** Loans are listings: never an offer, never requested in the app. */
describe('loans', () => {
  beforeEach(() => {
    search.kind = 'loan'
  })

  test('show an indicative range, say they are not an offer, and send the farmer to the office', () => {
    render(<EquipmentListScreen />)

    const card = screen.getByTestId('loan-item')
    expect(card).toHaveTextContent('Input loan')
    expect(card).toHaveTextContent('Seed and fertiliser for a season.')
    expect(within(card).getByTestId('loan-range')).toHaveTextContent('TZS 200,000.00 – TZS 1,500,000.00')
    expect(within(card).getByTestId('loan-range')).toHaveTextContent(/indicative/i)
    expect(card).toHaveTextContent(/ask at the ruaha office/i)
    expect(screen.getByTestId('loan-note')).toHaveTextContent(/not an offer/i)
  })

  test('cannot be requested in the app', () => {
    render(<EquipmentListScreen />)

    expect(within(screen.getByTestId('loan-item')).queryByRole('link')).toBeNull()
    expect(within(screen.getByTestId('loan-item')).queryByRole('button')).toBeNull()
  })

  test('none listed is an empty state', () => {
    useLoanProducts.mockReturnValue({ isLoading: false, error: null, items: [] })
    render(<EquipmentListScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })
})
