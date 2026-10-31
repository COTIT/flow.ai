import { useTranslation } from 'react-i18next'

import { SignOutButton } from '@/app/SignOutButton'
import { EmptyState } from '@/components/EmptyState'

/**
 * A real screen with a next step, never a blank page or a crash (spec 4.4).
 *
 * It is also never an error. Holding no membership is an ordinary state — a new
 * account nobody has attached to a project yet — so it gets the empty state's
 * sunken sand and dashed ring, and says who can fix it.
 */
export function NoAccessScreen() {
  const { t } = useTranslation()

  return (
    <section className="mx-auto flex w-full max-w-md flex-col gap-3.5" data-testid="no-access">
      <div className="flex flex-col gap-1">
        <h1 className="type-screen-title">{t('noAccess.title')}</h1>
        <p style={{ fontSize: 14, lineHeight: 1.55, color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('noAccess.detail')}
        </p>
      </div>

      <EmptyState title={t('noAccess.nothingTitle')} detail={t('noAccess.nothingDetail')} />

      <SignOutButton className="w-full" />
    </section>
  )
}
