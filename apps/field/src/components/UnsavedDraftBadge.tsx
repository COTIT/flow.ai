import { useTranslation } from 'react-i18next'

/**
 * An unsaved write must LOOK unsaved (business-rules §12).
 *
 * Shown whenever a form has reached IndexedDB but not the server. There is
 * deliberately no success toast for a local save — reaching local storage is
 * not a server write.
 *
 * Solid ground, red edge, filled dot. This badge is read on a phone that has
 * just lost signal halfway through a registration, with someone waiting; a
 * treatment that looked like success there would be a lie.
 */
export function UnsavedDraftBadge() {
  const { t } = useTranslation()

  return (
    <span
      data-testid="unsaved-draft-badge"
      className="type-note inline-flex items-center gap-2 px-2.5 py-1 font-medium"
      style={{
        border: '1.5px solid var(--flag-ink)',
        borderRadius: 'var(--radius-pill)',
        background: 'var(--paper)',
        color: 'var(--flag-ink)',
      }}
    >
      <span
        data-mark="unsaved"
        aria-hidden
        style={{
          width: 8,
          height: 8,
          borderRadius: 'var(--radius-pill)',
          background: 'var(--flag-ink)',
          flex: 'none',
        }}
      />
      {t('draft.notSubmitted')}
    </span>
  )
}
