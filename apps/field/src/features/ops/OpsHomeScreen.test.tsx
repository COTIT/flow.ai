import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useOpsHome = vi.fn()
const flags = vi.hoisted(() => ({ resources: true }))
vi.mock('@/lib/features', () => ({
  get resourcesEnabled() {
    return flags.resources
  },
}))
vi.mock('@/features/ops/useOpsHome', () => ({ useOpsHome: () => useOpsHome() }))
vi.mock('@tanstack/react-router', () => ({
  Link: ({
    children,
    to,
    search,
    ...rest
  }: {
    children: React.ReactNode
    to: string
    search?: Record<string, string>
  }) => (
    <a href={search ? `${to}?${new URLSearchParams(search)}` : to} {...rest}>
      {children}
    </a>
  ),
}))

const { OpsHomeScreen } = await import('@/features/ops/OpsHomeScreen')
await import('@/i18n')

beforeEach(() => {
  useOpsHome.mockReset()
  flags.resources = true
})

const queues = (over: Record<string, unknown> = {}) => ({
  awaitingReview: 2,
  openDemands: 1,
  outstandingRecords: 14,
  ...over,
})

/** Spec 7.1 — queue counts: requests awaiting review, open demands, unverified records. */
describe('OpsHomeScreen states', () => {
  test('loading shows a loading state', () => {
    useOpsHome.mockReturnValue({ isLoading: true, error: null, data: undefined })
    render(<OpsHomeScreen />)

    expect(screen.getByTestId('ops-home-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('ops-home')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    useOpsHome.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: undefined,
      refetch: vi.fn(),
    })
    render(<OpsHomeScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByText('could not reach the database')).toBeInTheDocument()
  })

  // An empty queue is good news, not an empty state: the screen still renders
  // its three counters, each reading zero.
  test('empty queues still render as zeroes, not as an empty state', () => {
    useOpsHome.mockReturnValue({
      isLoading: false,
      error: null,
      data: queues({ awaitingReview: 0, openDemands: 0, outstandingRecords: 0 }),
    })
    render(<OpsHomeScreen />)

    expect(screen.getByTestId('ops-home')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
    expect(screen.getByTestId('ops-queue-requests')).toHaveTextContent('0')
  })
})

describe('OpsHomeScreen content', () => {
  test('shows all three queue counts', () => {
    useOpsHome.mockReturnValue({ isLoading: false, error: null, data: queues() })
    render(<OpsHomeScreen />)

    expect(screen.getByTestId('ops-queue-requests')).toHaveTextContent('2')
    expect(screen.getByTestId('ops-queue-demands')).toHaveTextContent('1')
    expect(screen.getByTestId('ops-queue-verification')).toHaveTextContent('14')
  })

  // A count an ops user cannot act on is a statistic. This screen exists to
  // start work, so every figure is a link to the list behind it.
  test('every count links to the list it belongs to', () => {
    useOpsHome.mockReturnValue({ isLoading: false, error: null, data: queues() })
    render(<OpsHomeScreen />)

    expect(screen.getByTestId('ops-queue-requests')).toHaveAttribute(
      'href',
      '/ops/requests?status=submitted',
    )
    expect(screen.getByTestId('ops-queue-demands')).toHaveAttribute('href', '/ops/demand')
    // Verification is the officer's queue, and ops may read it —
    // SURFACE_ROLES.officer includes ops and admin.
    expect(screen.getByTestId('ops-queue-verification')).toHaveAttribute(
      'href',
      '/officer/verify',
    )
  })
})

// 25 Oct 2026: the request pipeline is held back from the first release.
test('with the resource flag off, the requests queue is not shown', () => {
  flags.resources = false
  useOpsHome.mockReturnValue({ isLoading: false, error: null, data: queues() })
  render(<OpsHomeScreen />)

  expect(screen.queryByTestId('ops-queue-requests')).not.toBeInTheDocument()
  expect(screen.getByTestId('ops-queue-demands')).toBeInTheDocument()
})
