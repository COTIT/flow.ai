import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useFarmerOpportunities = vi.fn()
vi.mock('@/features/farmer/useFarmerOpportunities', () => ({
  useFarmerOpportunities: () => useFarmerOpportunities(),
}))

const { FarmerOpportunitiesScreen } = await import(
  '@/features/farmer/FarmerOpportunitiesScreen'
)
await import('@/i18n')

beforeEach(() => useFarmerOpportunities.mockReset())

const opportunity = (over: Record<string, unknown> = {}) => ({
  id: 'o1',
  status: 'proposed',
  crop_name: 'Maize',
  offered_quantity_kg: 6400,
  my_contribution_kg: 4100,
  ...over,
})

/**
 * Spec 6.6 — opportunities this farmer's supply is inside.
 *
 * "Must state plainly that an opportunity is not a sale." That is a product
 * requirement, not copy preference: `accepted` means both sides agreed to keep
 * talking, and nothing has moved.
 */
describe('FarmerOpportunitiesScreen states', () => {
  test('loading shows a loading state, not an empty message', () => {
    useFarmerOpportunities.mockReturnValue({ isLoading: true, error: null, data: [] })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getByTestId('farmer-opportunities-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: [],
      refetch: vi.fn(),
    })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })

  // A farmer whose harvest nobody has committed yet is the common case, not a
  // failure. RLS returning zero rows means the same thing.
  test('no opportunities is a real message, not an error', () => {
    useFarmerOpportunities.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('FarmerOpportunitiesScreen content', () => {
  test('shows the crop and what this farmer contributed', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity()],
    })
    render(<FarmerOpportunitiesScreen />)

    const row = screen.getByTestId('farmer-opportunity')
    expect(row).toHaveTextContent('Maize')
    expect(row).toHaveTextContent('4,100.00 kg')
  })

  // `demand_read` is staff-only, so a farmer cannot read buyer_demand at all —
  // no buyer name, no demand window. The first version of this screen asked
  // for them anyway and rendered a blank name and "— – —" for the window.
  // The screen now says where that information lives instead.
  test('does not promise buyer details RLS does not grant a farmer', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity()],
    })
    render(<FarmerOpportunitiesScreen />)

    const row = screen.getByTestId('farmer-opportunity')
    expect(row).not.toHaveTextContent('— – —')
    expect(row).toHaveTextContent(/field officer|programme team/i)
  })

  // The farmer's own contribution is the figure that concerns them; the
  // opportunity's total is context. Showing only the total would overstate
  // what this farmer has promised.
  test('distinguishes this farmer contribution from the opportunity total', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity()],
    })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getByTestId('my-contribution')).toHaveTextContent('4,100.00 kg')
    expect(screen.getByTestId('offered-total')).toHaveTextContent('6,400.00 kg')
  })

  test('states plainly that an opportunity is not a sale', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity()],
    })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getByTestId('farmer-opportunities-note')).toHaveTextContent(/not a sale/i)
  })

  // Accepted is the state most likely to be misread as a completed deal.
  test('accepted is explained as an agreement to keep talking, not a sale', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity({ status: 'accepted' })],
    })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getByTestId('farmer-opportunities-note')).toHaveTextContent(/not a sale/i)
    expect(screen.getByTestId('farmer-opportunity')).not.toHaveTextContent(/sold|paid|delivered/i)
  })

  test('shows a status pill for each opportunity', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity(), opportunity({ id: 'o2', status: 'declined' })],
    })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.getAllByTestId('farmer-opportunity')).toHaveLength(2)
    expect(screen.getAllByTestId('status-pill')).toHaveLength(2)
  })

  // Read-only: an opportunity is created and moved by ops, never by a farmer.
  test('offers no controls', () => {
    useFarmerOpportunities.mockReturnValue({
      isLoading: false,
      error: null,
      data: [opportunity()],
    })
    render(<FarmerOpportunitiesScreen />)

    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })
})
