import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, test, vi } from 'vitest'
import { sessionStore } from '@genesy/api-client/session'

const authLogin = vi.fn()
const farmLoginsLogin = vi.fn()
const sessionFetch = vi.fn()
const navigate = vi.fn()
/** The route's search params. Mutated by the redirect tests. */
let search: { redirect?: string } = {}

vi.mock('@genesy/api-client/endpoints/auth', () => ({ authLogin: (a: unknown) => authLogin(a) }))
vi.mock('@genesy/api-client/endpoints/farm-logins', () => ({
  farmLoginsLogin: (a: unknown) => farmLoginsLogin(a),
}))
vi.mock('@/app/session', () => ({
  // The real registry entry, so the cache behaves as it does in the app.
  sessionQuery: { queryKey: ['session'], queryFn: () => sessionFetch(), staleTime: 0 },
}))
vi.mock('@tanstack/react-router', () => ({
  getRouteApi: () => ({ useSearch: () => search }),
  useNavigate: () => navigate,
}))

const { LoginScreen } = await import('@/app/LoginScreen')
const { clearTempPassword, peekTempPassword } = await import('@/app/tempPassword')
await import('@/i18n')

const WORKSPACE = '20000000-0000-4000-8000-000000000001'
const STAFF_TOKEN = { access_token: 'staff-tok', token_type: 'Bearer', workspace_ids: [WORKSPACE] }
const farmSession = (must_change_password: boolean) => ({
  access_token: 'farm-tok',
  token_type: 'Bearer',
  must_change_password,
  memberships: [
    { id: 'm1', workspace_id: WORKSPACE, role: 'farmer' },
    { id: 'm2', workspace_id: WORKSPACE, role: 'farmer' },
  ],
})
const failed = (status: number, data: unknown = {}) => ({ response: { status, data } })
const OPS_MEMBERSHIPS = {
  memberships: [{ id: 'm', role: 'ops', project_id: 'p', village_id: null, revoked_at: null }],
}

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

async function signInAs(id: string, password: string) {
  const user = userEvent.setup()
  await user.type(screen.getByTestId('login-email'), id)
  await user.type(screen.getByTestId('login-password'), password)
  await user.click(screen.getByTestId('login-submit'))
  return user
}

/** Every key and value in both browser stores, as one string to search. */
function browserStorage() {
  const dump: string[] = []
  for (const store of [localStorage, sessionStorage]) {
    for (let i = 0; i < store.length; i++) {
      const key = store.key(i) ?? ''
      dump.push(key, store.getItem(key) ?? '')
    }
  }
  return dump.join('\n')
}

beforeEach(() => {
  authLogin.mockReset()
  farmLoginsLogin.mockReset()
  sessionFetch.mockReset()
  navigate.mockReset()
  search = {}
  clearTempPassword()
  sessionStore.getState().signOut()
  localStorage.clear()
  sessionStorage.clear()
  authLogin.mockResolvedValue({ data: STAFF_TOKEN })
  farmLoginsLogin.mockResolvedValue({ data: farmSession(false) })
})

/**
 * Credentials can be accepted and the follow-up session read still fail. If
 * that is swallowed the user is left staring at the login form with no
 * explanation, having just typed a correct password.
 */
test('a session read that fails after a correct password is reported, not swallowed', async () => {
  sessionFetch.mockRejectedValue(failed(500, { message: 'could not connect to the database' }))

  renderLogin()
  await signInAs('ops@demo.ruaha360.test', 'demo1234')

  expect(await screen.findByTestId('login-error')).toHaveTextContent(
    'could not connect to the database',
  )
  expect(navigate).not.toHaveBeenCalled()
})

test('the submit button is usable again after a failure', async () => {
  sessionFetch.mockRejectedValue(failed(500, { message: 'boom' }))

  renderLogin()
  await signInAs('ops@demo.ruaha360.test', 'demo1234')

  await screen.findByTestId('login-error')
  expect(screen.getByTestId('login-submit')).toBeEnabled()
})

test('a successful sign-in navigates to the resolved landing route', async () => {
  sessionFetch.mockResolvedValue(OPS_MEMBERSHIPS)

  renderLogin()
  await signInAs('ops@demo.ruaha360.test', 'demo1234')

  await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: '/ops', replace: true }))
})

// Staff type an email (Genesy sign-in); farmers type the phone their officer
// registered (farm sign-in, which normalises it server side).
describe('which sign-in the typed id goes to', () => {
  test('a phone number signs in as a farmer, with the phone as typed', async () => {
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    await signInAs('0712 345 678', 'k7qxm2pa')

    await waitFor(() =>
      expect(farmLoginsLogin).toHaveBeenCalledWith({ phone: '0712 345 678', password: 'k7qxm2pa' }),
    )
    expect(authLogin).not.toHaveBeenCalled()
    expect(sessionStore.getState()).toMatchObject({
      token: 'farm-tok',
      // One per workspace, though the farmer holds two memberships in it.
      workspaceIds: [WORKSPACE],
      workspaceId: WORKSPACE,
    })
  })

  test('an email signs in as staff', async () => {
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    await signInAs('ops@demo.ruaha360.test', 'demo1234')

    await waitFor(() =>
      expect(authLogin).toHaveBeenCalledWith({ email: 'ops@demo.ruaha360.test', password: 'demo1234' }),
    )
    expect(farmLoginsLogin).not.toHaveBeenCalled()
    expect(sessionStore.getState().token).toBe('staff-tok')
  })
})

/**
 * The forced change sends the temporary password as current_password, so the
 * farmer is not asked for it twice. It is held in memory only: a shared phone
 * must not keep it in browser storage.
 */
describe('the temporary password', () => {
  test('is held in memory for the forced change, and never written to browser storage', async () => {
    farmLoginsLogin.mockResolvedValue({ data: farmSession(true) })
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    await signInAs('0712 345 678', 'k7qxm2pa')

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(peekTempPassword()).toBe('k7qxm2pa')
    // The session token is persisted; the password is not.
    expect(browserStorage()).toContain('farm-tok')
    expect(browserStorage()).not.toContain('k7qxm2pa')
  })

  test('is not held when no change is required', async () => {
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    await signInAs('0712 345 678', 'k7qxm2pa')

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(peekTempPassword()).toBeNull()
  })

  test('a staff password is never held', async () => {
    sessionFetch.mockResolvedValue(OPS_MEMBERSHIPS)

    renderLogin()
    await signInAs('ops@demo.ruaha360.test', 'demo1234')

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(peekTempPassword()).toBeNull()
    expect(browserStorage()).not.toContain('demo1234')
  })
})

// A guard that bounced someone here attached where they were going.
describe('the redirect after sign-in', () => {
  test('a same-app path is honoured', async () => {
    search = { redirect: '/officer/people' }
    sessionFetch.mockResolvedValue(OPS_MEMBERSHIPS)

    renderLogin()
    await signInAs('ops@demo.ruaha360.test', 'demo1234')

    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({ to: '/officer/people', replace: true }),
    )
  })

  test.each(['https://evil.test/', '//evil.test', '/login'])(
    'an unsafe target %s falls back to the role home',
    async (redirect) => {
      search = { redirect }
      sessionFetch.mockResolvedValue(OPS_MEMBERSHIPS)

      renderLogin()
      await signInAs('ops@demo.ruaha360.test', 'demo1234')

      await waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: '/ops', replace: true }))
    },
  )
})

// ── validation: the idle -> invalid edge ────────────────────
describe('field validation', () => {
  const noCall = () => {
    expect(authLogin).not.toHaveBeenCalled()
    expect(farmLoginsLogin).not.toHaveBeenCalled()
  }

  test('an empty form reports both fields and never reaches the network', async () => {
    renderLogin()
    await userEvent.setup().click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-email-error')).toBeInTheDocument()
    expect(screen.getByTestId('login-password-error')).toBeInTheDocument()
    noCall()
  })

  test('a malformed email is reported without a round trip', async () => {
    renderLogin()
    await signInAs('not-an-email', 'demo1234')

    expect(await screen.findByTestId('login-email-error')).toBeInTheDocument()
    noCall()
  })

  test('a missing password alone is reported', async () => {
    renderLogin()
    const user = userEvent.setup()
    await user.type(screen.getByTestId('login-email'), 'ops@demo.ruaha360.test')
    await user.click(screen.getByTestId('login-submit'))

    expect(await screen.findByTestId('login-password-error')).toBeInTheDocument()
    expect(screen.queryByTestId('login-email-error')).not.toBeInTheDocument()
    noCall()
  })

  test('fields are marked invalid for assistive tech, not just visually', async () => {
    renderLogin()
    await userEvent.setup().click(screen.getByTestId('login-submit'))

    await screen.findByTestId('login-email-error')
    expect(screen.getByTestId('login-email')).toHaveAttribute('aria-invalid', 'true')
    expect(screen.getByTestId('login-email')).toHaveAccessibleDescription(/email/i)
  })
})

// ── the error branches of a real attempt ───────────────────
describe('sign-in failure states', () => {
  test('bad credentials get the credentials message, not a raw status', async () => {
    farmLoginsLogin.mockRejectedValue(failed(401, { message: 'Unauthenticated.' }))

    renderLogin()
    await signInAs('0712 345 678', 'wrong')

    expect(await screen.findByTestId('login-error')).toHaveTextContent(
      'That phone number or email and password do not match an account.',
    )
    expect(screen.getByTestId('login-error')).not.toHaveTextContent('Unauthenticated')
  })

  test('staff bad credentials get the same message', async () => {
    authLogin.mockRejectedValue(failed(401))

    renderLogin()
    await signInAs('neema@demo.ruaha360.test', 'wrong')

    expect(await screen.findByTestId('login-error')).toHaveTextContent(
      'That phone number or email and password do not match an account.',
    )
  })

  // A thrown rejection with no HTTP answer is the connection, not the user.
  test('an unreachable server is distinguished from bad credentials', async () => {
    authLogin.mockRejectedValue(new Error('Network Error'))

    renderLogin()
    await signInAs('neema@demo.ruaha360.test', 'demo1234')

    expect(await screen.findByTestId('login-error')).toHaveTextContent(/connection/i)
  })

  test('any other failure is surfaced as the API wrote it', async () => {
    farmLoginsLogin.mockRejectedValue(
      failed(429, { message: 'Too many attempts. Ask your officer for a new password.' }),
    )

    renderLogin()
    await signInAs('0712 345 678', 'demo1234')

    expect(await screen.findByTestId('login-error')).toHaveTextContent(
      'Too many attempts. Ask your officer for a new password.',
    )
  })

  test('a field error is the most specific message, so it wins', async () => {
    authLogin.mockRejectedValue(
      failed(422, { message: 'The given data was invalid.', errors: { email: ['Unknown email.'] } }),
    )

    renderLogin()
    await signInAs('neema@demo.ruaha360.test', 'demo1234')

    expect(await screen.findByTestId('login-error')).toHaveTextContent('Unknown email.')
  })

  // A message the app names exactly is shown in the user's language, like every other screen.
  test("an API message the app knows is shown from the bundle", async () => {
    authLogin.mockRejectedValue(
      failed(422, { message: 'phone number must be a Tanzanian mobile number, for example +255 712 345 678' }),
    )

    renderLogin()
    await signInAs('neema@demo.ruaha360.test', 'demo1234')

    expect(await screen.findByTestId('login-error')).toHaveTextContent(
      'The phone number must be a Tanzanian mobile number, for example +255 712 345 678.',
    )
  })

  test('an answer with no message still says something', async () => {
    authLogin.mockRejectedValue(failed(500))

    renderLogin()
    await signInAs('neema@demo.ruaha360.test', 'demo1234')

    expect(await screen.findByTestId('login-error')).toHaveTextContent('Something went wrong signing in.')
  })

  test('a stale error clears when the next attempt starts', async () => {
    authLogin.mockRejectedValueOnce(failed(401))
    sessionFetch.mockResolvedValue({ memberships: [] })

    renderLogin()
    const user = await signInAs('neema@demo.ruaha360.test', 'wrong')
    await screen.findByTestId('login-error')

    await user.clear(screen.getByTestId('login-password'))
    await user.type(screen.getByTestId('login-password'), 'demo1234')
    await user.click(screen.getByTestId('login-submit'))

    await waitFor(() => expect(screen.queryByTestId('login-error')).not.toBeInTheDocument())
  })
})

// ── loading ────────────────────────────────────────────────
describe('submitting state', () => {
  test('the button is disabled and relabelled while in flight', async () => {
    let reject: (e: unknown) => void = () => {}
    authLogin.mockReturnValue(new Promise((_, r) => (reject = r)))

    renderLogin()
    await signInAs('ops@demo.ruaha360.test', 'demo1234')

    await waitFor(() => expect(screen.getByTestId('login-submit')).toBeDisabled())
    expect(screen.getByTestId('login-submit')).toHaveTextContent(/signing in/i)

    reject(failed(401))
    await waitFor(() => expect(screen.getByTestId('login-submit')).toBeEnabled())
  })
})

/**
 * QA #30. Every route guard calls `ensureSession`, so by the time the form is
 * submitted the session query already holds `null`. A cached read would send
 * a legitimate ops user to "You do not have access".
 */
describe('the session read after a correct password', () => {
  const OPS = {
    userId: 'u1',
    appUser: { id: 'u1', person_id: null },
    memberships: [
      { id: 'm1', role: 'ops', project_id: WORKSPACE, village_id: null, revoked_at: null },
    ],
  }

  test('ignores the signed-out answer left in the cache', async () => {
    sessionFetch.mockResolvedValue(OPS)
    renderLogin({ cachedSession: null })

    await signInAs('ops@demo.ruaha360.test', 'demo1234')

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(navigate).toHaveBeenCalledWith({ to: '/ops', replace: true })
  })

  test('and actually re-reads it rather than trusting the cache', async () => {
    sessionFetch.mockResolvedValue(OPS)
    renderLogin({ cachedSession: null })

    await signInAs('ops@demo.ruaha360.test', 'demo1234')

    await waitFor(() => expect(sessionFetch).toHaveBeenCalled())
  })

  // A user who genuinely holds no membership still belongs on /no-access.
  test('a user with no memberships still goes to no-access', async () => {
    sessionFetch.mockResolvedValue({ ...OPS, memberships: [] })
    renderLogin({ cachedSession: null })

    await signInAs('nobody@demo.ruaha360.test', 'demo1234')

    await waitFor(() => expect(navigate).toHaveBeenCalled())
    expect(navigate).toHaveBeenCalledWith({ to: '/no-access', replace: true })
  })
})
