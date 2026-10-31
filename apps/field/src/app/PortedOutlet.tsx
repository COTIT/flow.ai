import { Outlet, useLocation } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { isPorted } from '@/lib/ported'

/** A surface's outlet: the screen when it is on the Genesy API, otherwise "not available yet". */
export function PortedOutlet() {
  const { t } = useTranslation()
  const { pathname } = useLocation()
  if (isPorted(pathname)) return <Outlet />
  return (
    <div data-testid="not-yet-ported">
      <EmptyState title={t('notYet.title')} detail={t('notYet.detail')} />
    </div>
  )
}
