import { useQuery, type QueryClient } from '@tanstack/react-query'

import type { ActiveMembership } from '@/app/membership'
import { supabase } from '@/lib/supabase'
import { isTransientError } from '@/lib/errors'
import { queryKeys } from '@/lib/queryKeys'
import type { Database } from '@/lib/db.types'

type AppUserRow = Database['public']['Tables']['app_user']['Row']

export interface AppSession {
  userId: string
  email: string | null
  appUser: Pick<
    AppUserRow,
    'id' | 'person_id' | 'display_name' | 'locale' | 'must_change_password'
  > | null
  memberships: ActiveMembership[]
}

/**
 * auth user -> app_user -> the caller's own active memberships (spec §13).
 *
 * Returns null when signed out. That is an answer, not an error.
 */
export async function fetchSession(): Promise<AppSession | null> {
  const {
    data: { session },
  } = await supabase.auth.getSession()
  if (!session) return null

  const userId = session.user.id

  const [appUserResult, membershipResult] = await Promise.all([
    supabase
      .from('app_user')
      .select('id, person_id, display_name, locale, must_change_password')
      .eq('id', userId)
      .maybeSingle(),
    // `user_id` MUST be filtered here. membership carries two read policies:
    // membership_read_self and membership_read_managers, and the second lets
    // an ops user read every membership in the project. Without this filter an
    // ops login would see six rows, and resolveLanding would send them to the
    // role picker instead of their own surface.
    supabase
      .from('membership')
      .select('id, role, project_id, village_id, revoked_at')
      .eq('user_id', userId)
      .is('revoked_at', null),
  ])

  // A FAILED read is not an empty result. Coercing either error to a default
  // would make resolveLanding send a legitimate officer to /no-access, so a
  // transient blip would present as a permissions problem. Zero rows is an
  // answer; a broken query is not.
  if (appUserResult.error) throw new Error(appUserResult.error.message)
  if (membershipResult.error) throw new Error(membershipResult.error.message)

  const appUser = appUserResult.data ?? null
  const memberships = membershipResult.data ?? []

  // Every account that can sign in has an app_user row: seed_user creates one,
  // and app_register_farmer never creates a login. So an auth session with
  // neither an app_user nor any membership is not "this user has no access" —
  // it is a read that did not run as the user, which RLS reports as zero rows
  // rather than as an error. Treating it as no-access tells a legitimate
  // officer they have been removed from the programme.
  if (!appUser && memberships.length === 0) {
    throw new Error('Your account could not be read. Try again.')
  }

  return {
    userId,
    email: session.user.email ?? null,
    appUser,
    memberships,
  }
}

/**
 * A farmer still on the temporary password from their login card. Every
 * surface guard sends them to /set-password first. Convenience, not security:
 * the database refuses their survey answers until the flag clears.
 */
export function passwordChangeRequired(session: AppSession | null): boolean {
  return session?.appUser?.must_change_password === true
}

export const sessionQuery = {
  queryKey: queryKeys.session(),
  queryFn: fetchSession,
  // A revoked membership takes effect on the next query; RLS re-evaluates
  // every statement, so no session invalidation is needed.
  staleTime: 0,
  /**
   * The one query in the app that retries — QA #33.
   *
   * It runs immediately after a correct password, so its failure is the
   * worst-placed one here: it strands the user on a login form having just
   * proved who they are. And one of its failures is not their fault at all.
   * GoTrue can mint a token a fraction ahead of the clock that validates it,
   * so the FIRST request carrying it comes back `JWT issued at future`; a
   * second later the same token is fine.
   *
   * Twice, not forever, and only for the two conditions `isTransientError`
   * recognises. Anything the database decided — a policy, a constraint, an
   * account that cannot be read — surfaces at once, because repeating it
   * repeats the answer and delays the message.
   */
  retry: (failureCount: number, error: unknown) =>
    failureCount < 2 && isTransientError(error),
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
  await supabase.auth.signOut()
  queryClient.clear()
}
