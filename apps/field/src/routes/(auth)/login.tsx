import { createFileRoute, redirect } from '@tanstack/react-router'

import { LoginScreen } from '@/app/LoginScreen'
import { resolveLanding, safeRedirect } from '@/app/membership'
import { ensureSession } from '@/app/session'

export const Route = createFileRoute('/(auth)/login')({
  // Genuinely optional, so `navigate({ to: '/login' })` needs no search
  // object. A required-but-undefined property would force every caller to
  // pass one.
  validateSearch: (search: Record<string, unknown>): { redirect?: string } =>
    typeof search.redirect === 'string' ? { redirect: search.redirect } : {},
  /**
   * QA #26. `/login` rendered the sign-in form AND the header's "Sign out"
   * button for a user who was already signed in — one route showing two
   * mutually exclusive states. `resolveLanding` already does this everywhere
   * else.
   */
  beforeLoad: async ({ context, search }) => {
    const session = await ensureSession(context.queryClient)
    if (!session) return

    // A signed-in user arriving with ?redirect= was sent here by a guard and
    // then signed in elsewhere; honour where they were going.
    const requested = safeRedirect(search.redirect)
    throw redirect(
      requested ? { href: requested } : { to: resolveLanding(session.memberships).to },
    )
  },
  component: LoginScreen,
})
