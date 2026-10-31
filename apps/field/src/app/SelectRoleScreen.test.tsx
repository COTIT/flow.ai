import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

import type { ActiveMembership } from '@/app/membership'

const useSession = vi.fn()
const useScopeNames = vi.fn()
const navigate = vi.fn()

vi.mock('@/app/session', () => ({ useSession: () => useSession() }))
vi.mock('@/app/scope', () => ({ useScopeNames: () => useScopeNames() }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => navigate }))

const { SelectRoleScreen } = await import('@/app/SelectRoleScreen')
await import('@/i18n')

const VILLAGE = '30000000-0000-4000-8000-000000000001'
const PROJECT = '20000000-0000-4000-8000-000000000001'

const m = (role: ActiveMembership['role'], village: string | null = null): ActiveMembership => ({
  id: `m-${role}`,
  role,
  project_id: PROJECT,
  village_id: village,
  revoked_at: null,
})

function renderScreen() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <SelectRoleScreen />
    </QueryClientProvider>,
  )
}

const names = {
  data: { villages: { [VILLAGE]: 'Ilundo' }, projects: { [PROJECT]: 'Ruaha Ilundo Programme — DEMO' } },
  isLoading: false,
  error: null,
}

beforeEach(() => {
  useSession.mockReset()
  useScopeNames.mockReset()
  navigate.mockReset()
  useScopeNames.mockReturnValue(names)
})

describe('SelectRoleScreen loading state', () => {
  test('shows a loading state rather than an empty list while the session resolves', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true, error: null })
    renderScreen()

    expect(screen.getByTestId('select-role-loading')).toBeInTheDocument()
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
  })

  test('waits for the village and project names too, so rows do not pop in', () => {
    useSession.mockReturnValue({ data: { memberships: [m('farmer', VILLAGE)] }, isLoading: false, error: null })
    useScopeNames.mockReturnValue({ data: undefined, isLoading: true, error: null })
    renderScreen()

    expect(screen.getByTestId('select-role-loading')).toBeInTheDocument()
  })
})

describe('SelectRoleScreen error state', () => {
  test('a failed session read is an error, not an empty list', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: false, error: new Error('offline') })
    renderScreen()

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
    expect(screen.getByText('offline')).toBeInTheDocument()
  })

  test('a failed name lookup is an error too', () => {
    useSession.mockReturnValue({ data: { memberships: [m('farmer', VILLAGE)] }, isLoading: false, error: null })
    useScopeNames.mockReturnValue({ data: undefined, isLoading: false, error: new Error('village read failed') })
    renderScreen()

    expect(screen.getByTestId('error-state')).toBeInTheDocument()
  })
})

describe('SelectRoleScreen empty state', () => {
  // Reaching this screen with nothing to pick should not be a blank page. It
  // means the memberships were revoked between landing and rendering.
  test('no active memberships renders an empty state, not a blank page', () => {
    useSession.mockReturnValue({ data: { memberships: [] }, isLoading: false, error: null })
    renderScreen()

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  test('revoked memberships are not offered', () => {
    useSession.mockReturnValue({
      data: { memberships: [{ ...m('ops'), revoked_at: '2026-09-01T00:00:00Z' }] },
      isLoading: false,
      error: null,
    })
    renderScreen()

    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })
})

describe('SelectRoleScreen content', () => {
  // Spec 4.3: "Lists memberships as role + project + village."
  test('each row names the role, the project and the village', () => {
    useSession.mockReturnValue({
      data: { memberships: [m('field_officer', VILLAGE)] },
      isLoading: false,
      error: null,
    })
    renderScreen()

    const row = screen.getByTestId('select-role-field_officer')
    expect(row).toHaveTextContent('Field officer')
    expect(row).toHaveTextContent('Ruaha Ilundo Programme — DEMO')
    expect(row).toHaveTextContent('Ilundo')
  })

  // village_id NULL means whole-project scope, which is ops and admin.
  test('a project-wide membership says so instead of naming a village', () => {
    useSession.mockReturnValue({ data: { memberships: [m('ops', null)] }, isLoading: false, error: null })
    renderScreen()

    const row = screen.getByTestId('select-role-ops')
    expect(row).toHaveTextContent('Operations')
    expect(row).toHaveTextContent(/whole project/i)
  })

  test('choosing a role navigates to that role home', async () => {
    useSession.mockReturnValue({
      data: { memberships: [m('farmer', VILLAGE), m('ops', null)] },
      isLoading: false,
      error: null,
    })
    renderScreen()

    const { default: userEvent } = await import('@testing-library/user-event')
    await userEvent.click(screen.getByTestId('select-role-ops'))
    expect(navigate).toHaveBeenCalledWith({ to: '/ops', replace: true })
  })

  test('a village whose name has not loaded falls back to the id, never to blank', () => {
    useSession.mockReturnValue({
      data: { memberships: [m('farmer', 'unknown-village-id')] },
      isLoading: false,
      error: null,
    })
    renderScreen()

    expect(screen.getByTestId('select-role-farmer')).toHaveTextContent('unknown-village-id')
  })
})
