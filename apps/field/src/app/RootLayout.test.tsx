import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

import type { ActiveMembership } from '@/app/membership'

const useSession = vi.fn()
const pathname = vi.fn()

vi.mock('@/app/session', () => ({ useSession: () => useSession(), signOut: vi.fn() }))
vi.mock('@/app/LanguageSwitch', () => ({ LanguageSwitch: () => <div data-testid="language-switch" /> }))
// The shell as it is once every screen is on the Genesy API (the port gates are tested in ported.test.ts).
vi.mock('@/lib/ported', () => ({ BUG_REPORTS_PORTED: true, isPorted: () => true }))
vi.mock('@tanstack/react-router', () => ({
  Link: ({ children, to }: { children: React.ReactNode; to: string }) => <a href={to}>{children}</a>,
  Outlet: () => <div data-testid="outlet" />,
  useLocation: () => ({ pathname: pathname() }),
  useNavigate: () => vi.fn(),
}))

const { RootLayout } = await import('@/app/RootLayout')
await import('@/i18n')

const m = (role: ActiveMembership['role']): ActiveMembership => ({
  id: `m-${role}`,
  role,
  project_id: 'p',
  village_id: null,
  revoked_at: null,
})

function renderLayout() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={queryClient}>
      <RootLayout />
    </QueryClientProvider>,
  )
}

beforeEach(() => {
  useSession.mockReset()
  pathname.mockReset()
  pathname.mockReturnValue('/ops')
})

describe('RootLayout loading state', () => {
  // Rendering an empty nav while the session resolves would flash a bar with
  // no items and then fill it in. Render no nav until it is known.
  test('no nav is rendered while the session is resolving', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    renderLayout()

    expect(screen.queryByTestId('nav-sidebar')).not.toBeInTheDocument()
    expect(screen.queryByTestId('nav-tabs')).not.toBeInTheDocument()
    // The page itself still renders — the shell is not blocked on the session.
    expect(screen.getByTestId('outlet')).toBeInTheDocument()
  })

  test('no sign out is offered while the session is unknown', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    renderLayout()
    expect(screen.queryByTestId('sign-out')).not.toBeInTheDocument()
  })

  test('no bug report button until someone is signed in', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    renderLayout()
    expect(screen.queryByTestId('bug-fab')).not.toBeInTheDocument()
  })

  test('the demo banner shows regardless of session state', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    renderLayout()
    expect(screen.getByTestId('demo-banner')).toBeInTheDocument()
  })
})

describe('RootLayout signed-in state', () => {
  test('ops gets the sidebar and a sign out', () => {
    useSession.mockReturnValue({
      data: { appUser: { display_name: 'Asha Ops' }, memberships: [m('ops')] },
      isLoading: false,
    })
    renderLayout()

    expect(screen.getByTestId('nav-sidebar')).toBeInTheDocument()
    expect(screen.queryByTestId('nav-tabs')).not.toBeInTheDocument()
    expect(screen.getByTestId('sign-out')).toBeInTheDocument()
    expect(screen.getByTestId('current-user')).toHaveTextContent('Asha Ops')
    expect(screen.getByTestId('bug-fab')).toHaveAccessibleName('Report a problem')
  })

  test('a farmer on the farmer surface gets tabs', () => {
    pathname.mockReturnValue('/farm')
    useSession.mockReturnValue({
      data: { appUser: { display_name: 'Neema' }, memberships: [m('farmer')] },
      isLoading: false,
    })
    renderLayout()

    expect(screen.getByTestId('nav-tabs')).toBeInTheDocument()
    expect(screen.queryByTestId('nav-sidebar')).not.toBeInTheDocument()
  })

  test('an auth route gets no surface nav even when signed in', () => {
    pathname.mockReturnValue('/select-role')
    useSession.mockReturnValue({
      data: { appUser: { display_name: 'Neema' }, memberships: [m('farmer')] },
      isLoading: false,
    })
    renderLayout()

    expect(screen.queryByTestId('nav-tabs')).not.toBeInTheDocument()
    expect(screen.queryByTestId('nav-sidebar')).not.toBeInTheDocument()
  })

  test('a user without a display name still renders the shell', () => {
    useSession.mockReturnValue({ data: { appUser: null, memberships: [m('ops')] }, isLoading: false })
    renderLayout()

    expect(screen.queryByTestId('current-user')).not.toBeInTheDocument()
    expect(screen.getByTestId('nav-sidebar')).toBeInTheDocument()
  })
})

/**
 * The bottom tab bar is fixed to the viewport, so `main` has to end above it.
 *
 * This broke once already and took the register form's only submit button with
 * it: a `lg:` padding shorthand added for the ops surface beat `pb-20` at a
 * desktop width, and the 60px bar sat on top of the button. Nothing in the
 * suite noticed until Playwright spent thirty seconds trying to click through
 * a navigation link.
 */
describe('the tab bar never covers the last control', () => {
  test('a field surface pads the content clear of the bar', () => {
    useSession.mockReturnValue({ data: { appUser: { display_name: 'Salima' }, memberships: [m('field_officer')] } })
    pathname.mockReturnValue('/officer/register')
    renderLayout()

    const main = screen.getByTestId('outlet').closest('main')!
    // The measured bar height, which already includes the safe-area offset the
    // bar floats by, plus a rem of breathing room. The 6rem is only the value
    // before the bar has published its own.
    expect(main.className).toContain('pb-[calc(var(--tab-bar-height,6rem)+1rem)]')
    expect(
      main.className,
      'a padding shorthand at any breakpoint would beat the bottom padding',
    ).not.toMatch(/lg:p-/)
  })

  test('the ops surface has no bar, and gets its own desktop padding', () => {
    useSession.mockReturnValue({ data: { appUser: { display_name: 'Asha' }, memberships: [m('ops')] } })
    pathname.mockReturnValue('/ops/tower')
    renderLayout()

    const main = screen.getByTestId('outlet').closest('main')!
    expect(main.className).toMatch(/lg:p-/)
    expect(screen.queryByTestId('nav-tabs')).not.toBeInTheDocument()
  })

  test('the desktop workspace is not capped to a narrow centered column', () => {
    useSession.mockReturnValue({ data: { appUser: { display_name: 'Asha' }, memberships: [m('ops')] } })
    pathname.mockReturnValue('/ops/demand')
    renderLayout()

    const workspace = screen.getByTestId('outlet').parentElement!
    expect(workspace).toHaveClass('w-full', 'min-w-0')
    expect(workspace).not.toHaveClass('max-w-screen-2xl')
  })

})

/**
 * The mobile header no longer stacks every action as its own full-width
 * button — the `sm`-and-up row is hidden below it, replaced by one trigger
 * that opens the same actions as a popup (spec: logo left, account menu
 * right). A signed-out visitor has no identity to fold behind a trigger, so
 * the language switch stays directly on the mobile row for them.
 */
describe('the mobile header collapses actions into one menu', () => {
  test('signed in: the full action row is hidden below sm, and a single trigger takes its place', () => {
    useSession.mockReturnValue({
      data: { appUser: { display_name: 'Asha' }, memberships: [m('ops')] },
      isLoading: false,
    })
    renderLayout()

    const desktopActions = screen.getByTestId('global-header-actions')
    expect(desktopActions.className).toMatch(/\bhidden\b/)
    expect(desktopActions.className).toMatch(/\bsm:flex\b/)
    expect(screen.getByTestId('user-menu-trigger')).toBeInTheDocument()
  })

  test('signed out: there is no user menu, but the mobile row still offers the language switch', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: false })
    renderLayout()

    expect(screen.queryByTestId('user-menu-trigger')).not.toBeInTheDocument()
    expect(screen.getAllByTestId('language-switch').length).toBeGreaterThan(0)
  })

  test('opening the trigger surfaces the name, language switch and sign out', async () => {
    useSession.mockReturnValue({
      data: { appUser: { id: 'u-1', display_name: 'Asha' }, memberships: [m('ops')] },
      isLoading: false,
    })
    pathname.mockReturnValue('/ops/tower')
    renderLayout()

    await userEvent.click(screen.getByTestId('user-menu-trigger'))

    expect(await screen.findByTestId('user-menu-popup')).toBeInTheDocument()
    expect(screen.getByTestId('user-menu-name')).toHaveTextContent('Asha')
    expect(screen.getAllByTestId('language-switch').length).toBeGreaterThan(0)
    // The desktop row's own sign-out is also in the DOM (hidden by CSS, not
    // removed), so this only asserts the popup's copy is now reachable too.
    expect(screen.getAllByTestId('sign-out').length).toBeGreaterThan(0)
  })
})

/**
 * The tour crosses screens, so the way back to it lives in the header beside
 * the language switch rather than on any one of them.
 */
describe('the header offers the tour', () => {
  test('on a surface that has one', () => {
    useSession.mockReturnValue({
      data: { appUser: { id: 'u-1', display_name: 'Asha' }, memberships: [m('ops')] },
    })
    pathname.mockReturnValue('/ops/tower')
    renderLayout()

    expect(screen.getByRole('banner')).toContainElement(screen.getByTestId('tour-restart'))
  })

  // Signed out, or on a path that belongs to no surface: there is no role to
  // give a tour of.
  test('and not before anyone has signed in', () => {
    useSession.mockReturnValue({ data: undefined, isLoading: true })
    pathname.mockReturnValue('/login')
    renderLayout()

    expect(screen.queryByTestId('tour-restart')).not.toBeInTheDocument()
  })
})
