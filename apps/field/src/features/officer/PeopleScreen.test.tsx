import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const usePeople = vi.fn()
const navigate = vi.fn()
let searchParams: Record<string, unknown> = {}

vi.mock('@/features/officer/usePeople', () => ({ usePeople: () => usePeople() }))
vi.mock('@/app/scope', () => ({
  useScopeNames: () => ({ data: { villages: { v1: 'Ilundo' }, projects: {} } }),
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => searchParams }),
  useNavigate: () => navigate,
}))

const { PeopleScreen } = await import('@/features/officer/PeopleScreen')
await import('@/i18n')

beforeEach(() => {
  usePeople.mockReset()
  navigate.mockReset()
  searchParams = {}
})

const person = (over: Record<string, unknown> = {}) => ({
  id: 'p1',
  given_name: 'Neema',
  family_name: 'Mwakalinga',
  phone: '+255700000101',
  village_id: 'v1',
  source: 'field_verified',
  verification: 'verified',
  confidence: 'high',
  captured_at: '2026-09-09T21:30:00Z',
  ...over,
})

/** Spec 5.3 — people list, searchable by name and filterable by verification. */
describe('PeopleScreen states', () => {
  test('loading shows a loading state', () => {
    usePeople.mockReturnValue({ isLoading: true, error: null, data: [] })
    render(<PeopleScreen />)
    expect(screen.getByTestId('people-loading')).toBeInTheDocument()
  })

  test('an error is an error, offering retry', () => {
    usePeople.mockReturnValue({
      isLoading: false,
      error: new Error('could not reach the database'),
      data: [],
      refetch: vi.fn(),
    })
    render(<PeopleScreen />)
    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })

  // A filter that matches nothing is an empty state, never an error — and RLS
  // returning zero rows is the same answer.
  test('no matches is an empty state, not an error', () => {
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [] })
    render(<PeopleScreen />)

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })
})

describe('PeopleScreen content', () => {
  test('lists people with village and provenance', () => {
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [person()] })
    render(<PeopleScreen />)

    const table = screen.getByTestId('people-table')
    expect(table.parentElement?.parentElement).toHaveClass('bg-paper', 'overflow-hidden', 'min-w-0')
    expect(table).toHaveTextContent('Neema Mwakalinga')
    expect(table).toHaveTextContent('Ilundo')
    expect(screen.getByTestId('provenance-badge')).toBeInTheDocument()
  })

  test('a missing phone renders as absent, not as an empty cell', () => {
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [person({ phone: null })] })
    render(<PeopleScreen />)

    expect(screen.getByTestId('people-table')).toHaveTextContent('—')
  })
})

describe('PeopleScreen filters', () => {
  test('the filter controls reflect the URL, not local state', () => {
    searchParams = { q: 'Neema', verification: 'verified' }
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [person()] })
    render(<PeopleScreen />)

    expect(screen.getByTestId('people-search')).toHaveValue('Neema')
    expect(screen.getByTestId('people-filter-verification')).toHaveTextContent('Verified')
  })

  // Filter state lives in the URL (spec §10), so changing a filter navigates
  // rather than setting component state — that is what makes a filtered view
  // shareable and reloadable.
  test('changing the verification filter navigates', async () => {
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [person()] })
    render(<PeopleScreen />)

    await userEvent.click(screen.getByTestId('people-filter-verification'))
    await userEvent.click(await screen.findByRole('option', { name: 'Unverified' }))

    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({
        to: '/officer/people',
        search: expect.objectContaining({ verification: 'unverified' }),
      }),
    )
  })

  // An unrecognised value in the URL degrades to the unfiltered view rather
  // than reaching PostgREST as an invalid enum member.
  test('a nonsense filter in the URL degrades to unfiltered', () => {
    searchParams = { verification: 'not-a-status' }
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [person()] })
    render(<PeopleScreen />)

    expect(screen.getByTestId('people-filter-verification')).toHaveTextContent('All')
    expect(screen.queryByTestId('error-state')).not.toBeInTheDocument()
  })

  test('clearing the search removes it from the URL rather than sending an empty one', async () => {
    searchParams = { q: 'N' }
    usePeople.mockReturnValue({ isLoading: false, error: null, data: [person()] })
    render(<PeopleScreen />)

    await userEvent.clear(screen.getByTestId('people-search'))

    expect(navigate).toHaveBeenCalledWith(
      expect.objectContaining({ search: expect.objectContaining({ q: undefined }) }),
    )
  })
})
