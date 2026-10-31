import { createFileRoute, redirect } from '@tanstack/react-router'

import { SetPasswordScreen } from '@/app/SetPasswordScreen'
import { resolveLanding } from '@/app/membership'
import { ensureSession, passwordChangeRequired } from '@/app/session'

export const Route = createFileRoute('/(auth)/set-password')({
  // Only a signed-in user on a temporary password belongs here; anyone else is
  // sent where they would otherwise have gone.
  beforeLoad: async ({ context, location }) => {
    const session = await ensureSession(context.queryClient)
    if (!session) {
      throw redirect({ to: '/login', search: { redirect: location.href } })
    }
    if (!passwordChangeRequired(session)) {
      throw redirect({ to: resolveLanding(session.memberships).to })
    }
  },
  component: SetPasswordScreen,
})
