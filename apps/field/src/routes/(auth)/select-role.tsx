import { createFileRoute, redirect } from '@tanstack/react-router'

import { SelectRoleScreen } from '@/app/SelectRoleScreen'
import { needsRoleChoice, resolveLanding } from '@/app/membership'
import { ensureSession } from '@/app/session'

export const Route = createFileRoute('/(auth)/select-role')({
  /**
   * QA #10. This screen told the Ilundo officer "You hold more than one role.
   * Pick the one you want to work in" above a SINGLE option. Nothing routes
   * here for a single-role user — `resolveLanding` sends them straight home —
   * so only a typed URL arrives, and it should be sent home rather than shown
   * a claim that is not true.
   */
  beforeLoad: async ({ context, location }) => {
    const session = await ensureSession(context.queryClient)
    if (!session) {
      throw redirect({ to: '/login', search: { redirect: location.href } })
    }
    if (!needsRoleChoice(session.memberships)) {
      throw redirect({ to: resolveLanding(session.memberships).to })
    }
  },
  component: SelectRoleScreen,
})
