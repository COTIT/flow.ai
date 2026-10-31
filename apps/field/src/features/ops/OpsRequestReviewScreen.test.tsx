import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useOpsRequest = vi.fn()
const useVillageEnergy = vi.fn()
const mutate = vi.fn()
const useReviewAction = vi.fn()

const actorName = vi.fn((_id: string | null | undefined): string | undefined => undefined)
vi.mock('@/lib/actorNames', () => ({ useActorName: (id: string | null | undefined) => actorName(id) }))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useParams: () => ({ requestId: 'r1' }) }),
  Link: ({ children, to, ...props }: { children: React.ReactNode; to?: string } & Record<string, unknown>) => (
    <a href={to ?? '#'} {...props}>{children}</a>
  ),
}))
vi.mock('@/features/ops/useOpsRequests', () => ({
  useOpsRequest: () => useOpsRequest(),
  useVillageEnergy: () => useVillageEnergy(),
  useReviewAction: () => useReviewAction(),
}))

const { OpsRequestReviewScreen } = await import('@/features/ops/OpsRequestReviewScreen')
await import('@/i18n')

const request = {
  source: 'farmer_reported', verification: 'unverified', confidence: null,
  captured_at: '2026-09-09T12:00:00Z', captured_by: null,
  id: 'r1', village_id: 'v1', applicant: 'Baraka Mgeni', village_name: 'Ilundo', farm_label: 'Kipande',
  equipment_name: 'Grain dryer', purpose: 'Dry maize', status: 'under_review', submitted_at: '2026-09-09T12:00:00Z',
  decision_note: null, decided_at: null,
  estimate: { rated_power_kw: 12, quantity: 1, hours_per_day: 4, days_per_week: 5, est_power_kw: 12, est_kwh_per_week: 240 },
}

beforeEach(() => {
  useOpsRequest.mockReset()
  useVillageEnergy.mockReset()
  useReviewAction.mockReset()
  mutate.mockReset()
  useOpsRequest.mockReturnValue({ isLoading: false, error: null, request, refetch: vi.fn() })
  useVillageEnergy.mockReturnValue({ isLoading: false, error: null, data: { capacity_kw: 500, capacity_basis: 'planned', headroom_kw: 489, prospective_peak_kw: 12, approved_peak_kw: 0, simultaneity_factor: 0.6 } })
  useReviewAction.mockReturnValue({ isPending: false, isError: false, mutate, reset: vi.fn() })
})

describe('OpsRequestReviewScreen', () => {
  test('says whether the farmer asked to rent or to buy', () => {
    useOpsRequest.mockReturnValue({ isLoading: false, error: null, request: { ...request, acquisition: 'rent' }, refetch: vi.fn() })
    render(<OpsRequestReviewScreen />)
    expect(screen.getByTestId('request-acquisition')).toHaveTextContent(/rent/i)
  })

  test('renders request details, decision actions, breadcrumbs, and loading/not-found/error states', () => {
    const { rerender } = render(<OpsRequestReviewScreen />)
    expect(screen.getByTestId('request-review')).toHaveTextContent('Baraka Mgeni')
    expect(screen.getByTestId('action-approve')).toBeInTheDocument()
    expect(screen.getByTestId('action-reject')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: /Back/ })).toHaveAttribute('href', '/ops/requests')

    useOpsRequest.mockReturnValue({ isLoading: true, error: null, request: null })
    rerender(<OpsRequestReviewScreen />)
    expect(screen.getByTestId('ops-review-loading')).toBeInTheDocument()

    useOpsRequest.mockReturnValue({ isLoading: false, error: null, request: null })
    rerender(<OpsRequestReviewScreen />)
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()

    useOpsRequest.mockReturnValue({ isLoading: false, error: new Error('Request failed'), request: null, refetch: vi.fn() })
    rerender(<OpsRequestReviewScreen />)
    expect(screen.getByRole('alert')).toHaveTextContent('Request failed')
  })

  test('requires a decision note and submits an approved action after keyboard entry', async () => {
    const user = userEvent.setup()
    render(<OpsRequestReviewScreen />)

    await user.click(screen.getByTestId('action-approve'))
    expect(screen.getByTestId('decision-note-error')).toBeInTheDocument()
    await user.type(screen.getByTestId('decision-note'), 'Capacity confirmed')
    await user.click(screen.getByTestId('action-approve'))
    expect(mutate).toHaveBeenCalledWith({ action: 'approve', note: 'Capacity confirmed' })
  })
})
