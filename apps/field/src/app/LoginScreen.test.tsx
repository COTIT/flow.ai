import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'

const signInWithPassword = vi.fn()
const sessionFetch = vi.fn()
const navigate = vi.fn()

vi.mock('@/lib/supabase', () => ({
  supabase: { auth: { signInWithPassword: (a: unknown) => signInWithPassword(a) } },
}))
vi.mock('@/app/session', () => ({
  // The real registry entry, so the cache behaves as it does in the app.
  sessionQuery: { queryKey: ['session'], queryFn: () => sessionFetch(), staleTime: 0 },
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => ({}) }),
  useNavigate: () => navigate,
}))

const { LoginScreen } = await import('@/app/LoginScreen')
await import('@/i18n')

function renderLogin(seed?: { cachedSession: unknown }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  // What the route guards leave behind: every `beforeLoad` calls
  // `ensureSession`, so by the time the form is submitted the cache already
  // holds an answer from when the visitor was signed OUT.
  if (seed) queryClient.setQueryData(['session'], seed.cachedSession)
  render(
    <QueryClientProvider client={queryClient}>
      <LoginScreen />
    </QueryClientProvider>,
  )
  return queryClient
}

beforeEach(() => {
  signInWithPassword.mockReset()
  sessionFetch.mockReset()
  navigate.mockReset()
})

/**
 * Credentials can be accepted and the follow-up session read still fail. If
 * that is swallowed the user is left staring at the login form with no
 * explanation, having just typed a correct password.
 */
test('a session read that fails after a correct password is reported, not swallowed', async () => {
  signInWithPassword.mockResolvedValue({ error: null })
  sessionFetch.mockImplementation(() => Promise.reject(new Error('could not connect to the database')))

  renderLogin()
  const user = userEvent.setup()
  await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
  await user.type(screen.getByTestId('login-password'), 'demo1234')
  await user.click(screen.getByTestId('login-submit'))

  expect(await screen.findByTestId('login-error')).toHaveTextContent(
    'could not connect to the database',
  )
  expect(navigate).not.toHaveBeenCalled()
})

test('the submit button is usable again after a failure', async () => {
  signInWithPassword.mockResolvedValue({ error: null })
  sessionFetch.mockImplementation(() => Promise.reject(new Error('boom')))

  renderLogin()
  const user = userEvent.setup()
  await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
  await user.type(screen.getByTestId('login-password'), 'demo1234')
  await user.click(screen.getByTestId('login-submit'))

  await screen.findByTestId('login-error')
  expect(screen.getByTestId('login-submit')).toBeEnabled()
})

test('a successful sign-in navigates to the resolved landing route', async () => {
  signInWithPassword.mockResolvedValue({ error: null })
  sessionFetch.mockResolvedValue({
    memberships: [{ id: 'm', role: 'ops', project_id: 'p', village_id: null, revoked_at: null }],
  })

  renderLogin()
  const user = userEvent.setup()
  await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
  await user.type(screen.getByTestId('login-password'), 'demo1234')
  await user.click(screen.getByTestId('login-submit'))

  expect(navigate).toHaveBeenCalledWith({ to: '/ops', replace: true })
})

/**
 * Farmers sign in with the phone number their officer registered; the login
 * is created by app_farmer_login_issue under a hidden address derived from it.
 */
describe('signing in with a phone number', () => {
  test('any spelling of the phone becomes the login address the database created', async () => {
    signInWithPassword.mockResolvedValue({ error: null })
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), '0712 345 678')
    await user.type(screen.getByTestId('login-password'), 'k7qxm2pa')
    await user.click(screen.getByTestId('login-submit'))

    await waitFor(() =>
      expect(signInWithPassword).toHaveBeenCalledWith({
        email: '255712345678@farmers.ruaha360.test',
        password: 'k7qxm2pa',
      }),
    )
  })

  test('staff still sign in with their email as typed', async () => {
    signInWithPassword.mockResolvedValue({ error: null })
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    await waitFor(() =>
      expect(signInWithPassword).toHaveBeenCalledWith({
        email: 'ops@demo.ruaha360.test',
        password: 'demo1234',
      }),
    )
  })
})

// ── validation: the idle -> invalid edge ────────────────────
describe('field validation', () => {
  test('an empty form reports both fields and never reaches the network', async () => {
    renderLogin()
    const user = userEvent.setup()
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-email-error')).toBeInTheDocument()
    expect(screen.getByTestId('login-password-error')).toBeInTheDocument()
    expect(signInWithPassword).not.toHaveBeenCalled()
  })

  test('a malformed email is reported without a round trip', async () => {
    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'not-an-email')
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-email-error')).toBeInTheDocument()
    expect(signInWithPassword).not.toHaveBeenCalled()
  })

  test('a missing password alone is reported', async () => {
    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-password-error')).toBeInTheDocument()
    expect(screen.queryByTestId('login-email-error')).not.toBeInTheDocument()
    expect(signInWithPassword).not.toHaveBeenCalled()
  })

  test('fields are marked invalid for assistive tech, not just visually', async () => {
    renderLogin()
    const user = userEvent.setup()
    await user.click(screen.getByTestId('login-submit'))

    await screen.findByTestId('login-email-error')
    expect(screen.getByTestId('login-email')).toHaveAttribute('aria-invalid', 'true')
    expect(screen.getByTestId('login-email')).toHaveAccessibleDescription(/email/i)
  })
})

// ── the error branches of a real attempt ───────────────────
describe('sign-in failure states', () => {
  test('bad credentials get the credentials message, not a raw status', async () => {
    signInWithPassword.mockResolvedValue({
      error: { status: 400, message: 'Invalid login credentials' },
    })

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'neema@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'wrong')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-error')).toHaveTextContent(
      'That phone number or email and password do not match an account.',
    )
  })

  test('an unreachable server is distinguished from bad credentials', async () => {
    signInWithPassword.mockResolvedValue({
      error: { status: undefined, message: 'Failed to fetch' },
    })

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'neema@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-error')).toHaveTextContent(/connection/i)
  })

  test('any other failure is surfaced verbatim', async () => {
    signInWithPassword.mockResolvedValue({
      error: { status: 429, message: 'Email rate limit exceeded' },
    })

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'neema@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-error')).toHaveTextContent('Email rate limit exceeded')
  })

  test('a thrown rejection is caught rather than escaping the handler', async () => {
    signInWithPassword.mockRejectedValue(new Error('socket hang up'))

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'neema@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-error')).toHaveTextContent('socket hang up')
  })

  test('a stale error clears when the next attempt starts', async () => {
    signInWithPassword.mockResolvedValueOnce({ error: { status: 400, message: 'nope' } })
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'neema@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'wrong')
    await user.click(screen.getByTestId('login-submit'))
    await screen.findByTestId('login-error')

    signInWithPassword.mockResolvedValueOnce({ error: null })
    await user.clear(screen.getByTestId('login-password'))
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    await waitFor(() => expect(screen.queryByTestId('login-error')).not.toBeInTheDocument())
  })
})

// ── loading ────────────────────────────────────────────────
describe('submitting state', () => {
  test('the button is disabled and relabelled while in flight', async () => {
    let release: (v: unknown) => void = () => {}
    signInWithPassword.mockReturnValue(new Promise((r) => (release = r)))

    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    await waitFor(() => expect(screen.getByTestId('login-submit')).toBeDisabled())
    expect(screen.getByTestId('login-submit')).toHaveTextContent(/signing in/i)

    release({ error: { status: 400, message: 'nope' } })
    await waitFor(() => expect(screen.getByTestId('login-submit')).toBeEnabled())
  })
})

/**
 * QA #30. Signing in intermittently landed on `/no-access` instead of the
 * user's own surface — always on the first sign-in of a cold session, always
 * green on a retry.
 *
 * The cause is not a race in the database. Every route guard calls
 * `ensureSession`, so by the time the form is submitted the session query
 * already holds `null` — the correct answer for a visitor who was signed out.
 * `invalidateQueries` marks that stale and STARTS a refetch, but does not wait
 * for it, and `ensureQueryData` returns cached data when there is any. `null`
 * is data. So `resolveLanding([])` ran on the signed-out answer and sent a
 * legitimate ops user to "You do not have access".
 */
describe('the session read after a correct password', () => {
  const OPS = {
    userId: 'u1',
    appUser: { id: 'u1', person_id: null },
    memberships: [
      {
        id: 'm1',
        role: 'ops',
        project_id: '20000000-0000-4000-8000-000000000001',
        village_id: null,
        revoked_at: null,
      },
    ],
  }

  test('ignores the signed-out answer left in the cache', async () => {
    signInWithPassword.mockResolvedValue({ error: null })
    sessionFetch.mockResolvedValue(OPS)
    renderLogin({ cachedSession: null })

    await userEvent.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
    await userEvent.type(screen.getByTestId('login-password'), 'demo1234')
    await userEvent.click(screen.getByTestId('login-submit'))

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(navigate).toHaveBeenCalledWith({ to: '/ops', replace: true })
  })

  test('and actually re-reads it rather than trusting the cache', async () => {
    signInWithPassword.mockResolvedValue({ error: null })
    sessionFetch.mockResolvedValue(OPS)
    renderLogin({ cachedSession: null })

    await userEvent.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
    await userEvent.type(screen.getByTestId('login-password'), 'demo1234')
    await userEvent.click(screen.getByTestId('login-submit'))

    await waitFor(() => expect(sessionFetch).toHaveBeenCalled())
  })

  // A user who genuinely holds no membership still belongs on /no-access.
  test('a user with no memberships still goes to no-access', async () => {
    signInWithPassword.mockResolvedValue({ error: null })
    sessionFetch.mockResolvedValue({ ...OPS, memberships: [] })
    renderLogin({ cachedSession: null })

    await userEvent.type(screen.getByTestId('login-email'), 'nobody@demo.ruaha360.test')
    await userEvent.type(screen.getByTestId('login-password'), 'demo1234')
    await userEvent.click(screen.getByTestId('login-submit'))

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(navigate).toHaveBeenCalledWith({ to: '/no-access', replace: true })
  })
})
