import { useQuery, type QueryClient } from '@tanstack/react-query'
import { sessionStore } from '@genesy/api-client/session'
import { authLogout } from '@genesy/api-client/endpoints/auth'

import type { ActiveMembership } from '@/app/membership'
import '@/lib/api'
import { getFarmMe, type FarmMe } from '@/lib/farmMe'
import { apiStatus } from '@/lib/apiError'
import { queryKeys } from '@/lib/queryKeys'

export interface AppSession {
  userId: string
  email: string | null
  appUser: {
    id: string
    person_id: string | null
    display_name: string
    locale: string | null
    must_change_password: boolean
  }
  memberships: ActiveMembership[]
  /** Workspace and village names, for labelling a membership's scope. */
  me: FarmMe
}

/**
 * token -> `GET /v1/farm/me`: who is signed in, their own live farm roles and
 * whether the temporary password still has to go.
 *
 * Returns null when signed out. That is an answer, not an error. A failed read
 * throws: coercing it to "no roles" would send a legitimate officer to
 * /no-access because of a network blip.
 */
export async function fetchSession(): Promise<AppSession | null> {
  if (!sessionStore.persist.hasHydrated()) await sessionStore.persist.rehydrate()
  if (!sessionStore.getState().token) return null

  const me = await getFarmMe()
  // A person per workspace; the farmer surface works in one workspace at a time.
  const personId = me.memberships.find((m) => m.person_id)?.person_id ?? null
  return {
    userId: me.user.id,
    email: me.user.email,
    appUser: {
      id: me.user.id,
      person_id: personId,
      display_name: me.user.name,
      locale: me.locale,
      must_change_password: me.must_change_password,
    },
    memberships: me.memberships.map((m) => ({
      id: m.id,
      role: m.role,
      project_id: m.workspace_id,
      village_id: m.village_id,
      revoked_at: null,
    })),
    me,
  }
}

/**
 * A farmer still on the temporary password from their login card. Every
 * surface guard sends them to /set-password first. Convenience, not security:
 * the API refuses their survey answers until the flag clears.
 */
export function passwordChangeRequired(session: AppSession | null): boolean {
  return session?.appUser?.must_change_password === true
}

export const sessionQuery = {
  queryKey: queryKeys.session(),
  queryFn: fetchSession,
  // A revoked membership takes effect on the next query; the API checks
  // roles on every request, so no session invalidation is needed.
  staleTime: 0,
  /**
   * The one query in the app that retries — QA #33. It runs right after a
   * correct password, so failing it strands the user on the login form having
   * just proved who they are. Twice, and only when no answer came back (the
   * connection, in a village with intermittent signal). Anything the API
   * decided surfaces at once: repeating it repeats the answer.
   */
  retry: (failureCount: number, error: unknown) =>
    failureCount < 2 && apiStatus(error) === undefined,
}

export function useSession() {
  return useQuery(sessionQuery)
}

/** For router beforeLoad, which runs outside React. */
export function ensureSession(queryClient: QueryClient) {
  return queryClient.ensureQueryData(sessionQuery)
}

/**
 * Sign out, without leaving requests behind that fire against a dead token.
 *
 * QA #13: 24 × 400 in the console, clustered around sign-out and role
 * switching. In-flight reads and a queued token refresh completed AFTER the
 * token was invalidated. Cosmetic — a clean load has zero failures — but a
 * noisy console during a demo is where a real error goes unnoticed.
 *
 * Order matters. Cancel first, so nothing is in flight when the token dies;
 * then clear, so nothing cached from that session is refetched by the next
 * render with no token at all.
 */
export async function signOut(queryClient: QueryClient) {
  await queryClient.cancelQueries()
  // Ends the token on the API; signed out here even if that call fails.
  await authLogout().catch(() => undefined)
  sessionStore.getState().signOut()
  queryClient.clear()
}
