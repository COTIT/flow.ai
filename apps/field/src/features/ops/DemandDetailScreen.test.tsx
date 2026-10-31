import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useDemand = vi.fn()
const useDemandMatches = vi.fn()
const mutate = vi.fn()
const useCreateOpportunity = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ demandId: 'd1' }) }),
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))
vi.mock('@/app/scope', () => ({
  useScopeNames: () => ({ data: { villages: { v1: 'Ilundo' } } }),
}))
vi.mock('@/features/ops/useDemand', () => ({
  useDemand: () => useDemand(),
  useDemandMatches: () => useDemandMatches(),
  useCreateOpportunity: () => useCreateOpportunity(),
}))

const { DemandDetailScreen } = await import('@/features/ops/DemandDetailScreen')
await import('@/i18n')

const demand = {
  id: 'd1', buyer_name: 'Iringa Grain Traders', crop_name: 'Maize', crop_id: 'c1',
  quantity_kg: 9000, window_start: '2026-09-01', window_end: '2026-09-30',
  indicative_price_per_kg: 780, currency: 'TZS', delivery_point: 'Warehouse', quality_note: null,
  status: 'open',
}

const baseQuery = () => ({ isLoading: false, error: null, demand, refetch: vi.fn() })

beforeEach(() => {
  useDemand.mockReset()
  useDemandMatches.mockReset()
  useCreateOpportunity.mockReset()
  mutate.mockReset()
  useDemand.mockReturnValue(baseQuery())
  useDemandMatches.mockReturnValue({ isLoading: false, error: null, data: [] })
  useCreateOpportunity.mockReturnValue({ isPending: false, isError: false, mutate, reset: vi.fn() })
})

describe('DemandDetailScreen', () => {
  test('covers loading, not-found, and query error states', () => {
    useDemand.mockReturnValue({ isLoading: true, error: null, demand: null })
    const { rerender } = render(<DemandDetailScreen />)
    expect(screen.getByTestId('demand-detail-loading')).toBeInTheDocument()

    useDemand.mockReturnValue({ isLoading: false, error: null, demand: null })
    rerender(<DemandDetailScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()

    useDemand.mockReturnValue({ isLoading: false, error: new Error('Demand failed'), demand: null, refetch: vi.fn() })
    rerender(<DemandDetailScreen />)
    expect(screen.getByRole('alert')).toHaveTextContent('Demand failed')
  })

  test('renders matching supply with breadcrumbs and a create-opportunity action', async () => {
    useDemandMatches.mockReturnValue({
      isLoading: false,
      error: null,
      data: [{ village_id: 'v1', available_kg: 5600, committed_kg: 6400, coverable_kg: 5600, coverage_pct: 62.2, opportunity_id: null, opportunity_status: null }],
    })
    const user = userEvent.setup()
    render(<DemandDetailScreen />)

    expect(screen.getByTestId('demand-detail')).toBeInTheDocument()
    expect(screen.getByRole('navigation', { name: 'Breadcrumb' })).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Back/ })).toHaveAttribute('href', '/ops/demand')
    expect(screen.getByTestId('match-row-v1')).toHaveTextContent('Ilundo')
    await user.click(screen.getByTestId('create-opportunity'))
    expect(mutate).toHaveBeenCalledWith({ villageId: 'v1', cropId: 'c1' })
  })

  test('states an empty supply result instead of hiding the demand', () => {
    render(<DemandDetailScreen />)
    expect(screen.getByTestId('no-matching-supply')).toBeInTheDocument()
    expect(screen.getByTestId('demand-detail')).toHaveTextContent('Iringa Grain Traders')
  })
})
