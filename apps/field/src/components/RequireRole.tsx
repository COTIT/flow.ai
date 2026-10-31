import { useTranslation } from 'react-i18next'

import { canAccessSurface, type Surface } from '@/app/membership'
import { useSession } from '@/app/session'
import { EmptyState } from '@/components/EmptyState'

/**
 * In-page counterpart to the route guards.
 *
 * When a surface is closed to the current session this renders an EMPTY state,
 * not an error: spec 13 is explicit that "a farmer who hand-types
 * /ops/requests gets the page shell and zero rows. That is correct behaviour,
 * not a hole." RLS would have returned nothing regardless — this just says so
 * in words instead of showing a blank page.
 */
export function RequireRole({
  surface,
  children,
}: {
  surface: Surface
  children: React.ReactNode
}) {
  const { t } = useTranslation()
  const { data: session, isLoading } = useSession()

  // Nothing is known yet. Showing "nothing to see" before the session resolves
  // would flash a false negative.
  if (isLoading) return null

  if (!canAccessSurface(session?.memberships ?? [], surface)) {
    return <EmptyState title={t('empty.noAccessToThis')} detail={t('empty.noAccessDetail')} />
  }

  return <>{children}</>
}
