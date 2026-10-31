import { useEffect, useId, useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { ALL_CHAPTERS } from '@/app/tour/tourPlan'
import { BUTTON_PRIMARY, BUTTON_SECONDARY } from '@/components/controlStyles'

/**
 * The way into the tour: the whole thing, or one module of it.
 *
 * A presenter shows the module in front of them, so the tour cannot be a single
 * line that has to be clicked through from the start. Drawn as a sheet over the
 * dimmed page, like the confirm dialog, with a hairline rather than a shadow.
 */
export function TourMenu({
  chapters,
  onPick,
  onClose,
}: {
  chapters: Array<{ id: string; titleKey: string }>
  onPick: (chapterId: string) => void
  onClose: () => void
}) {
  const { t } = useTranslation()
  const titleId = useId()
  const firstRef = useRef<HTMLButtonElement>(null)

  useEffect(() => {
    firstRef.current?.focus()
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onClose])

  return (
    <div
      data-testid="tour-menu-backdrop"
      className="fixed inset-0 z-50 flex items-end justify-center p-4 pb-[max(1rem,env(safe-area-inset-bottom))] sm:items-center"
      style={{ background: 'var(--scrim)' }}
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        data-testid="tour-menu"
        className="flex max-h-[85dvh] w-full max-w-md flex-col gap-3 overflow-y-auto p-4"
        style={{
          background: 'var(--paper)',
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-card)',
          color: 'var(--ink)',
        }}
        onMouseDown={(event) => event.stopPropagation()}
      >
        <h2 id={titleId} className="type-title">
          {t('tour.menu.title')}
        </h2>

        <button
          ref={firstRef}
          type="button"
          data-testid={`tour-chapter-${ALL_CHAPTERS}`}
          onClick={() => onPick(ALL_CHAPTERS)}
          style={{ ...BUTTON_PRIMARY, textAlign: 'left' }}
        >
          {t('tour.menu.all')}
        </button>

        <div className="flex flex-col gap-2">
          {chapters.map((chapter) => (
            <button
              key={chapter.id}
              type="button"
              data-testid={`tour-chapter-${chapter.id}`}
              onClick={() => onPick(chapter.id)}
              style={{ ...BUTTON_SECONDARY, textAlign: 'left' }}
            >
              {t(chapter.titleKey)}
            </button>
          ))}
        </div>

        <button
          type="button"
          data-testid="tour-menu-close"
          onClick={onClose}
          className="self-end underline"
          style={{
            background: 'none',
            border: 0,
            padding: '8px 0',
            fontSize: 14,
            fontFamily: 'inherit',
            color: 'var(--ink-3)',
            textUnderlineOffset: 3,
          }}
        >
          {t('tour.menu.close')}
        </button>
      </div>
    </div>
  )
}
