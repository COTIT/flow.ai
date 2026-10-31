import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const useOfficerHome = vi.fn()
const flags = vi.hoisted(() => ({ resources: true }))
vi.mock('@/lib/features', () => ({
  get resourcesEnabled() {
    return flags.resources
  },
}))
vi.mock('@/features/officer/useOfficerHome', () => ({ useOfficerHome: () => useOfficerHome() }))
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to, ...rest }: { children: React.ReactNode; to: string }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}))

const { OfficerHomeScreen } = await import('@/features/officer/OfficerHomeScreen')
await import('@/i18n')

beforeEach(() => {
  useOfficerHome.mockReset()
  flags.resources = true
})

const village = (over: Record<string, unknown> = {}) => ({
  villageId: '30000000-0000-4000-8000-000000000001',
  villageName: 'Ilundo',
  persons: 6,
  farms: 4,
  requests: 5,
  unverified: 9,
  ...over,
})

/**
 * Spec 5.1 names three states for this screen — loading, zero villages
 * assigned, and error — plus the content: assigned villages, the unverified
 * record count, and Register as the primary action.
 */
describe('OfficerHomeScreen states', () => {
  test('loading shows a loading state, not an empty message', () => {
    useOfficerHome.mockReturnValue({ isLoading: true, error: null, villages: [] })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('officer-home-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    useOfficerHome.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      villages: [],
      refetch: vi.fn(),
    })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByText('could not reach the database')).toBeInTheDocument()
    expect(screen.getByTestId('error-retry')).toBeInTheDocument()
  })

  // An officer with no membership village is a real situation — someone added
  // to the project but not yet to a village. It gets a message with a next
  // step, never an error and never a blank page.
  test('zero villages assigned is a real message, not a blank page', () => {
    useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [] })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('OfficerHomeScreen content', () => {
  test('renders the screen and its assigned villages', () => {
    useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [village()] })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('officer-home')).toBeInTheDocument()
    expect(screen.getByTestId('officer-village')).toHaveTextContent('Ilundo')
  })

  test('shows the counts spec 5.1 names: person, farm, request', () => {
    useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [village()] })
    render(<OfficerHomeScreen />)

    const card = screen.getByTestId('officer-village')
    expect(card).toHaveTextContent('6')
    expect(card).toHaveTextContent('4')
    expect(card).toHaveTextContent('5')
  })

  test('surfaces the unverified record count', () => {
    useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [village()] })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('officer-unverified')).toHaveTextContent('9')
  })

  // Verification is a deliberate act, so the count is a prompt to go and do
  // it, not a passive figure.
  test('the unverified count links to the verify queue', () => {
    useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [village()] })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('officer-unverified-link')).toHaveAttribute('href', '/officer/verify')
  })

  // "0 records still need verifying" is a worse sentence than saying the work
  // is done, so the screen must not render the count at all in this case — and
  // it must not offer a link to an empty queue.
  test('nothing outstanding says so rather than showing a bare zero', () => {
    useOfficerHome.mockReturnValue({
      isLoading: false,
      error: null,
      villages: [village({ unverified: 0 })],
    })
    render(<OfficerHomeScreen />)

    const outstanding = screen.getByTestId('officer-unverified')
    expect(outstanding).toHaveTextContent(/verified/i)
    expect(outstanding).not.toHaveTextContent(/\b0\b/)
    expect(screen.queryByTestId('officer-unverified-link')).not.toBeInTheDocument()
  })

  // Spec 5.1: "primary action Register".
  test('Register is the primary action', () => {
    useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [village()] })
    render(<OfficerHomeScreen />)

    expect(screen.getByTestId('officer-home-register')).toHaveAttribute(
      'href',
      '/officer/register',
    )
  })

  test('several assigned villages each get their own card', () => {
    useOfficerHome.mockReturnValue({
      isLoading: false,
      error: null,
      villages: [village(), village({ villageId: 'v2', villageName: 'Mgama', unverified: 2 })],
    })
    render(<OfficerHomeScreen />)

    expect(screen.getAllByTestId('officer-village')).toHaveLength(2)
    // The outstanding figure is the total across assigned villages: 9 + 2.
    expect(screen.getByTestId('officer-unverified')).toHaveTextContent('11')
  })
})

// 25 Oct 2026: equipment requests are held back from the first release.
test('with the resource flag off, the village card has no equipment-request count', () => {
  flags.resources = false
  useOfficerHome.mockReturnValue({ isLoading: false, error: null, villages: [village()] })
  render(<OfficerHomeScreen />)

  const card = screen.getByTestId('officer-village')
  expect(card).toHaveTextContent('Farms')
  expect(card).not.toHaveTextContent('Equipment requests')
})
