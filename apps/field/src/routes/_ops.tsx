import { Outlet, createFileRoute, redirect } from '@tanstack/react-router'

import { canAccessSurface, resolveLanding } from '@/app/membership'
import { ensureSession, passwordChangeRequired } from '@/app/session'

// Route guards are UX, not security. RLS is the boundary: anyone who reaches a
// surface anyway gets the page shell and zero rows, which is correct.
export const Route = createFileRoute('/_ops')({
  beforeLoad: async ({ context, location }) => {
    const session = await ensureSession(context.queryClient)
    if (!session) {
      throw redirect({ to: '/login', search: { redirect: location.href } })
    }
    if (passwordChangeRequired(session)) throw redirect({ to: '/set-password' })
    if (!canAccessSurface(session.memberships, 'ops')) {
      throw redirect({ to: resolveLanding(session.memberships).to })
    }
  },
  component: () => <Outlet />,
})
