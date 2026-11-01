import { createFileRoute, redirect } from '@tanstack/react-router'

import { resolveLanding } from '@/app/membership'
import { ensureSession, passwordChangeRequired } from '@/app/session'

export const Route = createFileRoute('/')({
  beforeLoad: async ({ context }) => {
    const session = await ensureSession(context.queryClient)
    if (!session) throw redirect({ to: '/login' })
    if (passwordChangeRequired(session)) throw redirect({ to: '/set-password', replace: true })
    throw redirect({ to: resolveLanding(session.memberships).to, replace: true })
  },
})
