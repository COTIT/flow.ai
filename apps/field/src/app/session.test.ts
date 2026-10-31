import { beforeEach, describe, expect, test, vi } from 'vitest'

const getSession = vi.fn()
const from = vi.fn()
const authSignOut = vi.fn()

vi.mock('@/lib/supabase', () => ({
  supabase: {
    auth: { getSession: () => getSession(), signOut: () => authSignOut() },
    from: (table: string) => from(table),
  },
}))

const { fetchSession, signOut, sessionQuery } = await import('@/app/session')

const USER = '80000000-0000-4000-8000-000000000003'

/** app_user: .select().eq().maybeSingle() */
function appUserChain(result: unknown) {
  return { select: () => ({ eq: () => ({ maybeSingle: async () => result }) }) }
}
/** membership: .select().eq().is() */
function membershipChain(result: unknown) {
  return { select: () => ({ eq: () => ({ is: async () => result }) }) }
}

function mockTables(appUser: unknown, membership: unknown) {
  from.mockImplementation((table: string) =>
    table === 'app_user' ? appUserChain(appUser) : membershipChain(membership),
  )
}

beforeEach(() => {
  getSession.mockReset()
  from.mockReset()
})

describe('fetchSession', () => {
  test('returns null when signed out, which is an answer not an error', async () => {
    getSession.mockResolvedValue({ data: { session: null } })
    await expect(fetchSession()).resolves.toBeNull()
  })

  test('resolves auth user, app_user and the caller own memberships', async () => {
    getSession.mockResolvedValue({
      data: { session: { user: { id: USER, email: 'officer@example.test' } } },
    })
    mockTables(
      { data: { id: USER, person_id: null, display_name: 'Salima Officer', locale: 'sw' }, error: null },
      { data: [{ id: 'm1', role: 'field_officer', project_id: 'p', village_id: 'v', revoked_at: null }], error: null },
    )

    const session = await fetchSession()
    expect(session?.userId).toBe(USER)
    expect(session?.email).toBe('officer@example.test')
    expect(session?.appUser?.display_name).toBe('Salima Officer')
    expect(session?.memberships).toHaveLength(1)
  })

  // Superseded by the not-yet-effective-token cases below: an app_user row is
  // what distinguishes "this user has no memberships" from "this read did not
  // run as the user", so a genuine no-access session must carry one.
  test('genuinely having no memberships resolves to an empty list', async () => {
    getSession.mockResolvedValue({ data: { session: { user: { id: USER, email: null } } } })
    mockTables(
      { data: { id: USER, person_id: null, display_name: 'New User', locale: 'en' }, error: null },
      { data: [], error: null },
    )

    const session = await fetchSession()
    expect(session?.memberships).toEqual([])
  })

  // The important one. A failed membership read is NOT "no memberships": if it
  // is swallowed, resolveLanding sends a legitimate officer to /no-access and
  // a transient blip looks like a permissions problem.
  test('a failed membership read throws instead of looking like no access', async () => {
    getSession.mockResolvedValue({ data: { session: { user: { id: USER, email: null } } } })
    mockTables(
      { data: null, error: null },
      { data: null, error: { message: 'network error', code: '' } },
    )

    await expect(fetchSession()).rejects.toThrow(/network error/)
  })

  test('a failed app_user read also throws', async () => {
    getSession.mockResolvedValue({ data: { session: { user: { id: USER, email: null } } } })
    mockTables(
      { data: null, error: { message: 'permission denied', code: '42501' } },
      { data: [], error: null },
    )

    await expect(fetchSession()).rejects.toThrow(/permission denied/)
  })
})

describe('fetchSession guards against a not-yet-effective token', () => {
  // Every account that can sign in has an app_user row — seed_user creates
  // one, and app_register_farmer never creates a login. So an auth session
  // with NO app_user row and NO memberships is not "this user has no access";
  // it is a read that did not run as the user, which RLS reports as zero rows
  // rather than as an error. Routing that to /no-access tells a legitimate
  // officer they have been removed from the programme.
  test('an auth session with no app_user and no memberships throws', async () => {
    getSession.mockResolvedValue({ data: { session: { user: { id: USER, email: null } } } })
    mockTables({ data: null, error: null }, { data: [], error: null })

    await expect(fetchSession()).rejects.toThrow(/could not be read/i)
  })

  test('an app_user with genuinely zero memberships is still no-access, not an error', async () => {
    getSession.mockResolvedValue({ data: { session: { user: { id: USER, email: null } } } })
    mockTables(
      { data: { id: USER, person_id: null, display_name: 'New User', locale: 'en' }, error: null },
      { data: [], error: null },
    )

    const session = await fetchSession()
    expect(session?.memberships).toEqual([])
    expect(session?.appUser?.display_name).toBe('New User')
  })

  test('an app_user with memberships resolves normally', async () => {
    getSession.mockResolvedValue({ data: { session: { user: { id: USER, email: null } } } })
    mockTables(
      { data: { id: USER, person_id: null, display_name: 'Salima', locale: 'sw' }, error: null },
      { data: [{ id: 'm1', role: 'field_officer', project_id: 'p', village_id: 'v', revoked_at: null }], error: null },
    )

    const session = await fetchSession()
    expect(session?.memberships).toHaveLength(1)
  })
})

/**
 * QA #13. 24 × 400 appeared in the console across the sweep, clustered around
 * sign-out and role switching: queries and an in-flight token refresh firing
 * against an already invalidated token.
 *
 * Cosmetic — a clean Tower load has 15 Supabase requests and zero failures —
 * but it makes the console noisy enough to hide a real error during a demo,
 * which is the one moment it matters.
 */
describe('signOut', () => {
  const makeClient = () => ({
    cancelQueries: vi.fn().mockResolvedValue(undefined),
    invalidateQueries: vi.fn().mockResolvedValue(undefined),
    clear: vi.fn(),
  })

  beforeEach(() => {
    // Two statements, and `mockImplementation` rather than chaining
    // `mockResolvedValue` onto `mockReset()` — the chained form leaves a
    // promise vitest reports as an unhandled rejection in the test that
    // overrides it.
    authSignOut.mockReset()
    authSignOut.mockImplementation(async () => ({ error: null }))
  })

  // In-flight reads are what fire again with a dead token. Cancelling first is
  // the whole fix.
  test('cancels in-flight queries before invalidating the token', async () => {
    const client = makeClient()
    const order: string[] = []
    client.cancelQueries.mockImplementation(async () => void order.push('cancel'))
    authSignOut.mockImplementation(async () => {
      order.push('signOut')
      return { error: null }
    })

    await signOut(client as never)

    expect(order).toEqual(['cancel', 'signOut'])
  })

  // Nothing cached belonged to the session that just ended. Leaving it means
  // the next render refetches it — with no token at all.
  test('drops the cache rather than refetching it signed out', async () => {
    const client = makeClient()
    await signOut(client as never)

    expect(client.clear).toHaveBeenCalledTimes(1)
  })

  test('the cache is dropped after the token is gone, not before', async () => {
    const client = makeClient()
    const order: string[] = []
    authSignOut.mockImplementation(async () => {
      order.push('signOut')
      return { error: null }
    })
    client.clear.mockImplementation(() => void order.push('clear'))

    await signOut(client as never)

    expect(order).toEqual(['signOut', 'clear'])
  })

  // A failed sign out must still surface: SignOutButton reports it, and a
  // button that silently did nothing is worse than an error.
  test('a failure still reaches the caller', async () => {
    const client = makeClient()
    authSignOut.mockImplementation(() => Promise.reject(new Error('network')))

    let caught: unknown
    try {
      await signOut(client as never)
    } catch (cause) {
      caught = cause
    }

    expect((caught as Error).message).toBe('network')
    // And the cache is NOT dropped: the user is still signed in.
    expect(client.clear).not.toHaveBeenCalled()
  })
})

/**
 * QA #33. The session read runs immediately after a correct password, so its
 * failure is the worst-placed one in the app: it strands the user on a login
 * form having just proved who they are.
 *
 * And one of its failures is not the user's fault at all — GoTrue can mint a
 * token a fraction ahead of the clock that validates it, so the first request
 * carrying it is rejected for skew. It was the flake that had the first test
 * of an e2e run sitting on /login until the assertion gave up.
 */
describe('the session query retry policy', () => {
  const shouldRetry = (failures: number, error: unknown) => {
    const retry = sessionQuery.retry
    return typeof retry === 'function' ? retry(failures, error as Error) : Boolean(retry)
  }

  test('tries again once when the token was issued a moment ahead', () => {
    expect(shouldRetry(0, new Error('JWT issued at future'))).toBe(true)
  })

  test('and when the connection dropped', () => {
    expect(shouldRetry(0, new TypeError('Failed to fetch'))).toBe(true)
  })

  // Twice, not forever: a condition that survives two attempts is not a blip,
  // and the user is waiting on a form.
  test('but gives up rather than spinning', () => {
    expect(shouldRetry(2, new Error('JWT issued at future'))).toBe(false)
  })

  /**
   * The rule this must not break. A read RLS refused, or an account that
   * genuinely cannot be read, has to surface at once — repeating it repeats
   * the same answer and delays the message.
   */
  test('never retries a decision the database made', () => {
    expect(shouldRetry(0, new Error('new row violates row-level security policy'))).toBe(false)
    expect(shouldRetry(0, new Error('Your account could not be read. Try again.'))).toBe(false)
  })

  // Zero rows is a success and never reaches a retry policy at all — asserted
  // so that nobody "fixes" this by widening it.
  test('an empty result is not an error and cannot be retried', async () => {
    mockTables({ data: null, error: null }, { data: [], error: null })
    getSession.mockResolvedValue({ data: { session: null }, error: null })

    await expect(sessionQuery.queryFn()).resolves.toBeNull()
  })
})

describe('passwordChangeRequired', () => {
  const base = { userId: 'u', email: null, memberships: [] }
  test('a farmer on a temporary password is sent to choose their own', async () => {
    const { passwordChangeRequired } = await import('@/app/session')
    expect(
      passwordChangeRequired({
        ...base,
        appUser: { id: 'u', person_id: 'p', display_name: 'N', locale: 'sw', must_change_password: true },
      }),
    ).toBe(true)
  })

  test('everyone else is not', async () => {
    const { passwordChangeRequired } = await import('@/app/session')
    expect(passwordChangeRequired(null)).toBe(false)
    expect(passwordChangeRequired({ ...base, appUser: null })).toBe(false)
    expect(
      passwordChangeRequired({
        ...base,
        appUser: { id: 'u', person_id: null, display_name: 'S', locale: 'en', must_change_password: false },
      }),
    ).toBe(false)
  })
})
