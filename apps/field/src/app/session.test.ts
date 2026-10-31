import { beforeEach, describe, expect, test, vi } from 'vitest'
import { sessionStore } from '@genesy/api-client/session'

import type { FarmMe } from '@/lib/farmMe'

const getFarmMe = vi.fn()
const authLogout = vi.fn()

vi.mock('@/lib/farmMe', () => ({ getFarmMe: () => getFarmMe() }))
vi.mock('@genesy/api-client/endpoints/auth', () => ({ authLogout: () => authLogout() }))

const { fetchSession, signOut, sessionQuery, passwordChangeRequired } = await import('@/app/session')
type AppSession = NonNullable<Awaited<ReturnType<typeof fetchSession>>>

const USER = '80000000-0000-4000-8000-000000000003'
const WORKSPACE = '20000000-0000-4000-8000-000000000001'

const signIn = () =>
  sessionStore.getState().signIn({ access_token: 'tok', token_type: 'Bearer', workspace_ids: [WORKSPACE] })

function farmMe(overrides: Partial<FarmMe> = {}): FarmMe {
  return {
    user: { id: USER, name: 'Salima Officer', email: 'officer@example.test', phone: null },
    locale: 'sw',
    must_change_password: false,
    memberships: [
      {
        id: 'm1',
        workspace_id: WORKSPACE,
        workspace_name: 'Ruaha',
        role: 'field_officer',
        village_id: 'v1',
        village_name: 'Idodi',
        person_id: null,
      },
    ],
    ...overrides,
  }
}

const failed = (status: number, message = 'refused') => ({ response: { status, data: { message } } })

beforeEach(() => {
  getFarmMe.mockReset()
  sessionStore.getState().signOut()
  localStorage.clear()
})

describe('fetchSession', () => {
  test('returns null when signed out, which is an answer not an error', async () => {
    await expect(fetchSession()).resolves.toBeNull()
    expect(getFarmMe).not.toHaveBeenCalled()
  })

  test('maps /farm/me to the user, their account fields and their own memberships', async () => {
    signIn()
    const me = farmMe({ must_change_password: true })
    getFarmMe.mockResolvedValue(me)

    const session = await fetchSession()
    expect(session).toEqual({
      userId: USER,
      email: 'officer@example.test',
      appUser: {
        id: USER,
        person_id: null,
        display_name: 'Salima Officer',
        locale: 'sw',
        must_change_password: true,
      },
      memberships: [
        { id: 'm1', role: 'field_officer', project_id: WORKSPACE, village_id: 'v1', revoked_at: null },
      ],
      me,
    })
  })

  test('a farmer carries the person their membership names', async () => {
    signIn()
    getFarmMe.mockResolvedValue(
      farmMe({
        memberships: [
          { ...farmMe().memberships[0], id: 'm1', role: 'farmer', person_id: null },
          { ...farmMe().memberships[0], id: 'm2', role: 'farmer', person_id: 'p-42' },
        ],
      }),
    )

    expect((await fetchSession())?.appUser.person_id).toBe('p-42')
  })

  test('genuinely having no memberships resolves to an empty list', async () => {
    signIn()
    getFarmMe.mockResolvedValue(farmMe({ memberships: [] }))

    const session = await fetchSession()
    expect(session?.memberships).toEqual([])
    expect(session?.appUser.display_name).toBe('Salima Officer')
  })

  // The important one. A failed read is NOT "no memberships": if it is
  // swallowed, resolveLanding sends a legitimate officer to /no-access and a
  // transient blip looks like a permissions problem.
  test('a failed read throws instead of looking like no access', async () => {
    signIn()
    const cause = failed(500, 'could not connect to the database')
    getFarmMe.mockRejectedValue(cause)

    await expect(fetchSession()).rejects.toBe(cause)
  })

  test('a lost connection also throws', async () => {
    signIn()
    getFarmMe.mockRejectedValue(new Error('Network Error'))

    await expect(fetchSession()).rejects.toThrow('Network Error')
  })
})

/**
 * QA #13. Requests firing against an already invalidated token, clustered
 * around sign-out: a noisy console during a demo is where a real error hides.
 */
describe('signOut', () => {
  const makeClient = () => ({
    cancelQueries: vi.fn().mockResolvedValue(undefined),
    clear: vi.fn(),
  })

  beforeEach(() => {
    authLogout.mockReset()
    authLogout.mockImplementation(async () => undefined)
    signIn()
  })

  // In-flight reads are what fire again with a dead token. Cancelling first is
  // the whole fix.
  test('cancels in-flight queries before ending the token', async () => {
    const client = makeClient()
    const order: string[] = []
    client.cancelQueries.mockImplementation(async () => void order.push('cancel'))
    authLogout.mockImplementation(async () => void order.push('logout'))

    await signOut(client as never)

    expect(order).toEqual(['cancel', 'logout'])
  })

  test('forgets the token, here and in localStorage', async () => {
    await signOut(makeClient() as never)

    expect(sessionStore.getState().token).toBeNull()
    expect(localStorage.getItem('genesy.session') ?? '').not.toContain('tok')
  })

  // Nothing cached belonged to the session that just ended.
  test('drops the cache rather than refetching it signed out', async () => {
    const client = makeClient()
    await signOut(client as never)

    expect(client.clear).toHaveBeenCalledTimes(1)
  })

  test('the cache is dropped after the token is gone, not before', async () => {
    const client = makeClient()
    const order: string[] = []
    authLogout.mockImplementation(async () => void order.push('logout'))
    client.clear.mockImplementation(() => {
      order.push(sessionStore.getState().token ? 'clear with token' : 'clear')
    })

    await signOut(client as never)

    expect(order).toEqual(['logout', 'clear'])
  })

  // The API call only ends the token server side. Failing it must not leave
  // the user signed in on this device.
  test('a failed logout call still signs out here', async () => {
    const client = makeClient()
    authLogout.mockImplementation(() => Promise.reject(new Error('Network Error')))

    await expect(signOut(client as never)).resolves.toBeUndefined()

    expect(sessionStore.getState().token).toBeNull()
    expect(client.clear).toHaveBeenCalledTimes(1)
  })
})

/**
 * QA #33. The session read runs immediately after a correct password, so its
 * failure strands the user on a login form having just proved who they are.
 */
describe('the session query retry policy', () => {
  const shouldRetry = (failures: number, error: unknown) => {
    const retry = sessionQuery.retry
    return typeof retry === 'function' ? retry(failures, error as Error) : Boolean(retry)
  }

  test('tries again when no answer came back', () => {
    expect(shouldRetry(0, new Error('Network Error'))).toBe(true)
    expect(shouldRetry(1, new TypeError('Failed to fetch'))).toBe(true)
  })

  // Twice, not forever: a condition that survives two attempts is not a blip,
  // and the user is waiting on a form.
  test('but gives up rather than spinning', () => {
    expect(shouldRetry(2, new Error('Network Error'))).toBe(false)
  })

  // Repeating a decision the API made repeats the answer and delays the message.
  test('never retries an answer the API gave', () => {
    for (const status of [401, 403, 404, 422, 500]) {
      expect(shouldRetry(0, failed(status))).toBe(false)
    }
  })

  // Signed out is a success and never reaches a retry policy at all.
  test('an empty result is not an error and cannot be retried', async () => {
    await expect(sessionQuery.queryFn()).resolves.toBeNull()
  })
})

describe('passwordChangeRequired', () => {
  const session = (must_change_password: boolean) =>
    ({
      userId: 'u',
      email: null,
      memberships: [],
      appUser: { id: 'u', person_id: 'p', display_name: 'N', locale: 'sw', must_change_password },
    }) as unknown as AppSession

  test('a farmer on a temporary password is sent to choose their own', () => {
    expect(passwordChangeRequired(session(true))).toBe(true)
  })

  test('everyone else is not', () => {
    expect(passwordChangeRequired(null)).toBe(false)
    expect(passwordChangeRequired(session(false))).toBe(false)
  })
})
