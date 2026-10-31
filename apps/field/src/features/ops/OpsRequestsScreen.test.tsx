import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useOpsRequests = vi.fn()
const useScopeNames = vi.fn()
const navigate = vi.fn()
const search = vi.fn()

vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => search() }),
  useNavigate: () => navigate,
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))
vi.mock('@/features/ops/useOpsRequests', () => ({ useOpsRequests: () => useOpsRequests() }))
vi.mock('@/app/scope', () => ({ useScopeNames: () => useScopeNames() }))

const { OpsRequestsScreen } = await import('@/features/ops/OpsRequestsScreen')
await import('@/i18n')

const request = {
  id: 'r1', applicant: 'Neema Mwakalinga', village_name: 'Ilundo', equipment_name: 'Cold room',
  status: 'approved', submitted_at: '2026-09-09T12:00:00Z', village_id: 'v1', estimate: { est_power_kw: 5 },
}

beforeEach(() => {
  useOpsRequests.mockReset()
  useScopeNames.mockReset()
  navigate.mockReset()
  search.mockReset()
  search.mockReturnValue({})
  useScopeNames.mockReturnValue({ data: { villages: { v1: 'Ilundo', v2: 'Mgama' } } })
  useOpsRequests.mockReturnValue({ isLoading: false, error: null, requests: [request], refetch: vi.fn() })
})

describe('OpsRequestsScreen', () => {
  test('shows whether each request is to rent or to buy', () => {
    useOpsRequests.mockReturnValue({ isLoading: false, error: null, requests: [{ ...request, acquisition: 'buy' }], refetch: vi.fn() })
    render(<OpsRequestsScreen />)
    expect(screen.getByTestId('requests-table')).toHaveTextContent(/to buy/i)
  })

  test('renders a white table surface with both custom filters and rows', () => {
    render(<OpsRequestsScreen />)

    expect(screen.getByRole('heading', { name: 'Request pipeline' })).toBeInTheDocument()
    expect(screen.getByTestId('filter-status')).toBeInTheDocument()
    expect(screen.getByTestId('filter-village')).toBeInTheDocument()
    expect(screen.getByTestId('requests-table')).toBeInTheDocument()
    expect(screen.getByTestId('requests-table').parentElement?.parentElement).toHaveClass('bg-paper')
  })

  test('writes status and village changes back to the URL', async () => {
    const user = userEvent.setup()
    render(<OpsRequestsScreen />)

    await user.click(screen.getByTestId('filter-status'))
    await user.click(await screen.findByRole('option', { name: 'Approved' }))
    expect(navigate).toHaveBeenCalledWith(expect.objectContaining({ to: '/ops/requests', replace: true }))

    await user.click(screen.getByTestId('filter-village'))
    await user.click(await screen.findByRole('option', { name: 'Ilundo' }))
    expect(navigate).toHaveBeenLastCalledWith(expect.objectContaining({ search: expect.objectContaining({ village: 'v1' }) }))
  })

  test('covers loading, error, and empty states', () => {
    useOpsRequests.mockReturnValue({ isLoading: true, error: null, requests: [] })
    const { rerender } = render(<OpsRequestsScreen />)
    expect(screen.getByTestId('ops-requests-loading')).toBeInTheDocument()

    useOpsRequests.mockReturnValue({ isLoading: false, error: new Error('Pipeline failed'), requests: [], refetch: vi.fn() })
    rerender(<OpsRequestsScreen />)
    expect(screen.getByRole('alert')).toHaveTextContent('Pipeline failed')

    useOpsRequests.mockReturnValue({ isLoading: false, error: null, requests: [] })
    rerender(<OpsRequestsScreen />)
    expect(screen.getByText('No requests match')).toBeInTheDocument()
  })

  test('keeps nested navigation keyboard reachable through a row', async () => {
    const user = userEvent.setup()
    render(<OpsRequestsScreen />)
    const row = screen.getByTestId('request-row')
    row.focus()
    await user.keyboard('{Enter}')
    expect(navigate).toHaveBeenCalledWith({ to: '/ops/requests/$requestId', params: { requestId: 'r1' } })
  })
})
