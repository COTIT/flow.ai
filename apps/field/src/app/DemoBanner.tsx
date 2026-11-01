import { useTranslation } from 'react-i18next'

import { isDemoData } from '@/lib/dataMode'

/**
 * Driven by VITE_DATA_MODE, never by a database column: the demo instance and
 * any future live instance are different databases, so there is no is_demo
 * flag to read.
 *
 * Full width, in the brand green, at the top of every page including the login
 * screen. It is the first thing a stakeholder sees and it has one job: nothing
 * below it is a measured Ruaha result.
 */
export function DemoBanner() {
  const { t } = useTranslation()
  if (!isDemoData) return null

  return (
    <div
      data-testid="demo-banner"
      className="flex flex-wrap items-center justify-center gap-2.5 px-4 py-2 text-center"
      style={{
        background: 'var(--accent)',
        color: '#1d2a06',
        fontSize: 13,
        lineHeight: 1.4,
      }}
    >
      <span
        className="type-microlabel px-2.5 py-px"
        style={{ border: '1px solid rgba(29, 42, 6, .35)', borderRadius: 'var(--radius-pill)' }}
      >
        {t('demoBanner.label')}
      </span>
      <span className="font-medium">{t('demoBanner.detail')}</span>
    </div>
  )
}
