import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useMyPassbook = vi.fn()
const usePersonPassbook = vi.fn()
const usePayouts = vi.fn()
vi.mock('@/features/passbook/usePassbook', () => ({
  useMyPassbook: () => useMyPassbook(),
  usePersonPassbook: (id: string) => usePersonPassbook(id),
  usePayouts: (id?: string) => usePayouts(id),
  useCreatePayout: () => ({ mutate: vi.fn(), reset: vi.fn(), isPending: false, error: null, isSuccess: false }),
}))
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to }: { children: React.ReactNode; to: string }) => <a href={to}>{children}</a>,
}))

const { MyPassbookScreen, MyPassbookCard, PersonPassbookSection } = await import('@/features/passbook/screens')

const tzs = (minor: number) => ({ minor, currency: 'TZS', decimals: 2, currency_id: 'tzs' })
const ok = <T,>(data: T) => ({ isLoading: false, error: null, data, refetch: vi.fn() })
const notFound = { isLoading: false, error: { isAxiosError: true, response: { status: 404, data: { message: 'Not found.' } } }, data: undefined, refetch: vi.fn() }
const credit = { id: 'l1', kind: 'sale_credit', amount: tzs(500000), balance_after: tzs(500000), attribution_id: null, payout_id: null, created_at: '2026-11-01T08:00:00Z' }

beforeEach(() => {
  vi.clearAllMocks()
  usePayouts.mockReturnValue(ok([]))
})

describe('the farmer', () => {
  test("reads their own passbook and payouts (the API scopes payouts to them)", () => {
    useMyPassbook.mockReturnValue(ok({ balances: [tzs(500000)], lines: [credit] }))
    render(<MyPassbookScreen />)
    expect(screen.getByTestId('passbook-balance-TZS')).toBeVisible()
    expect(usePayouts).toHaveBeenCalledWith(undefined)
    expect(screen.queryByTestId('pay-out-TZS')).toBeNull()
  })

  test('not a registered farmer: an empty state, not an error', () => {
    useMyPassbook.mockReturnValue(notFound)
    render(<MyPassbookScreen />)
    expect(screen.getByTestId('passbook-not-registered')).toBeVisible()
    expect(screen.queryByTestId('error-state')).toBeNull()
  })

  test('home card: the balance and a link to the passbook', () => {
    useMyPassbook.mockReturnValue(ok({ balances: [tzs(500000)], lines: [credit] }))
    render(<MyPassbookCard />)
    expect(screen.getByTestId('passbook-home-TZS')).toHaveTextContent('5,000.00')
    expect(screen.getByRole('link', { name: 'Open my passbook' })).toHaveAttribute('href', '/farm/passbook')
  })

  test('home card: nothing yet says so', () => {
    useMyPassbook.mockReturnValue(ok({ balances: [], lines: [] }))
    render(<MyPassbookCard />)
    expect(screen.getByTestId('passbook-home-empty')).toBeVisible()
  })

  test('home card: a failed read offers a retry', () => {
    useMyPassbook.mockReturnValue({ isLoading: false, error: new Error('down'), data: undefined, refetch: vi.fn() })
    render(<MyPassbookCard />)
    expect(screen.getByTestId('error-state')).toBeVisible()
  })

  test('home card: not registered hides the card', () => {
    useMyPassbook.mockReturnValue(notFound)
    const { container } = render(<MyPassbookCard />)
    expect(container).toBeEmptyDOMElement()
  })
})

describe('staff on a person page', () => {
  test("the person's passbook, payouts and a pay out per positive balance", () => {
    usePersonPassbook.mockReturnValue(ok({ balances: [tzs(500000)], lines: [credit] }))
    render(<PersonPassbookSection personId="p1" personName="Neema Mwakalinga" />)
    expect(usePersonPassbook).toHaveBeenCalledWith('p1')
    expect(usePayouts).toHaveBeenCalledWith('p1')
    expect(screen.getByTestId('pay-out-TZS')).toBeVisible()
  })
})
