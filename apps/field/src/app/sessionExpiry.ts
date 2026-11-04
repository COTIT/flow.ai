import type { QueryClient } from '@tanstack/react-query'
import { sessionStore } from '@genesy/api-client/session'

type Nav = { navigate: (to: { to: '/login'; search: { redirect: string }; replace: true }) => unknown; href: () => string }

/**
 * B5: a 401 mid-session signs out in the API client (`signOut("expired")`), which left the user on a
 * screen of failed reads. Go to sign-in, which says the session ended, and come back here after.
 * The user's own sign-out navigates by itself. Returns the unsubscribe.
 */
export function returnToSignInWhenExpired(queryClient: QueryClient, router: Nav): () => void {
  return sessionStore.subscribe((now, before) => {
    if (!before.token || now.token || !now.expired) return
    void queryClient.cancelQueries()
    queryClient.clear()
    void router.navigate({ to: '/login', search: { redirect: router.href() }, replace: true })
  })
}
