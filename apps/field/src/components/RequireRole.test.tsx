import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, test, vi } from 'vitest'

import type { ActiveMembership } from '@/app/membership'
import '@/i18n'

const useSession = vi.fn()
vi.mock('@/app/session', () => ({ useSession: () => useSession() }))

const { RequireRole } = await import('@/components/RequireRole')

const m = (role: ActiveMembership['role']): ActiveMembership => ({
  id: crypto.randomUUID(),
  role,
  project_id: 'p',
  village_id: null,
  revoked_at: null,
})

beforeEach(() => useSession.mockReset())

describe('RequireRole', () => {
  test('renders the surface when the session opens it', () => {
    useSession.mockReturnValue({ data: { memberships: [m('ops')] }, isLoading: false })
    render(
      <RequireRole surface="ops">
        <p>Request pipeline</p>
      </RequireRole>,
    )
    expect(screen.getByText('Request pipeline')).toBeInTheDocument()
  })

  // Spec 13: "A farmer who hand-types /ops/requests gets the page shell and
  // zero rows. That is correct behaviour, not a hole." So this renders an
  // EMPTY state, not an error — RLS would have returned nothing anyway.
  test('renders an empty state, not an error, when the surface is closed', () => {
    useSession.mockReturnValue({ data: { memberships: [m('farmer')] }, isLoading: false })
    render(
      <RequireRole surface="ops">
        <p>Request pipeline</p>
      </RequireRole>,
    )
    expect(screen.queryByText('Request pipeline')).not.toBeInTheDocument()
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  test('revoked roles close the surface', () => {
    useSession.mockReturnValue({
      data: { memberships: [{ ...m('ops'), revoked_at: '2026-09-01T00:00:00Z' }] },
      isLoading: false,
    })
    render(
      <RequireRole surface="ops">
        <p>Request pipeline</p>
      </RequireRole>,
    )
    expect(screen.getByTestId('empty-state')).toBeInTheDocument()
  })

  test('renders nothing while the session is still resolving', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    const { container } = render(
      <RequireRole surface="ops">
        <p>Request pipeline</p>
      </RequireRole>,
    )
    expect(screen.queryByTestId('empty-state')).not.toBeInTheDocument()
    expect(container.textContent).toBe('')
  })
})
