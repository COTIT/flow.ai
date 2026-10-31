import type { TooltipRenderProps } from 'react-joyride'
import { useTranslation } from 'react-i18next'

import { useTour } from '@/app/tour/tourContext'
import { BUTTON_PRIMARY, BUTTON_SECONDARY } from '@/components/controlStyles'

/**
 * The tour bubble, drawn by us.
 *
 * Joyride ships a tooltip of its own and it belongs to a different product: its
 * own type scale, its own blue, a drop shadow, a corner "×". Dropping that on
 * top of this design would make the tour the one surface in the app that came
 * from somewhere else — and it is the FIRST surface a new user reads, which is
 * the worst possible place for that.
 *
 * So the library keeps the two jobs only it can do — cutting the spotlight and
 * positioning the bubble against a moving target — and everything inside is the
 * ordinary design system: `--paper` on a `--rule` hairline, the shared control
 * surface for both buttons, the same type steps as every card, no shadow.
 *
 * A stop can carry, in `step.data`, the chapter it belongs to and whether the
 * person is invited to try the highlighted part. A stop can also be gated: Next
 * stays unavailable until the page has done what the stop asked, and the bubble
 * says so rather than leaving a dead button.
 */
export function TourTooltip({
  index,
  size,
  isLastStep,
  step,
  backProps,
  primaryProps,
  skipProps,
  tooltipProps,
}: TooltipRenderProps) {
  const { t } = useTranslation()
  const { gateOpen } = useTour()
  const data = (step.data ?? {}) as { chapterKey?: string; tryIt?: boolean }
  const waiting = !gateOpen

  return (
    <div
      {...tooltipProps}
      data-testid="tour-tooltip"
      className="flex max-w-sm flex-col gap-3 p-4"
      style={{
        background: 'var(--paper)',
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        // The bubble floats over content, and a hairline is what separates it
        // here. Everywhere else in this design a shadow would have done that
        // job, and everywhere else in this design there are no shadows.
        color: 'var(--ink)',
      }}
    >
      <div className="flex flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
          <span data-testid="tour-progress" className="type-microlabel" style={{ color: 'var(--ink-3)' }}>
            {t('tour.progress', { step: index + 1, total: size })}
          </span>
          {data.chapterKey && (
            <span data-testid="tour-chapter" className="type-microlabel" style={{ color: 'var(--ink-3)' }}>
              {t(data.chapterKey)}
            </span>
          )}
          {data.tryIt && (
            <span data-testid="tour-tryit" className="type-microlabel" style={{ color: 'var(--primary-ink)' }}>
              {t('tour.tryIt')}
            </span>
          )}
        </div>
        {/* A card title, not a section label: `.type-section` is 12px
            uppercase, and stacked under the uppercase step counter it made the
            bubble read as two labels and no heading. */}
        {step.title && (
          <h2 className="type-title" style={{ color: 'var(--ink)' }}>
            {step.title}
          </h2>
        )}
      </div>

      <p style={{ fontSize: 15, lineHeight: '22px', color: 'var(--ink-2)' }}>{step.content}</p>

      {waiting && (
        <p data-testid="tour-gate-hint" style={{ fontSize: 14, lineHeight: '20px', color: 'var(--primary-ink)' }}>
          {t('tour.gateHint')}
        </p>
      )}

      <div className="flex flex-wrap items-center justify-between gap-2.5">
        {/* Nothing left to skip on the last stop, and "skip" there would only
            mean "finish" — two controls for one action. */}
        {isLastStep ? (
          <span />
        ) : (
          <button
            {...skipProps}
            type="button"
            data-testid="tour-skip"
            className="underline"
            style={{
              background: 'none',
              border: 0,
              padding: 0,
              fontSize: 14,
              fontFamily: 'inherit',
              color: 'var(--ink-3)',
              textUnderlineOffset: 3,
            }}
          >
            {t('tour.skip')}
          </button>
        )}

        <div className="flex flex-wrap items-center gap-2">
          {/* Back is meaningless on the first stop, and a control that is
              disabled until it is not is furniture in the meantime. */}
          {index > 0 && (
            <button {...backProps} type="button" data-testid="tour-back" style={BUTTON_SECONDARY}>
              {t('tour.back')}
            </button>
          )}
          <button
            {...primaryProps}
            type="button"
            data-testid="tour-next"
            disabled={waiting}
            style={
              waiting
                ? { ...BUTTON_PRIMARY, background: 'var(--sand-2)', color: 'var(--ink-3)' }
                : BUTTON_PRIMARY
            }
          >
            {isLastStep ? t('tour.close') : t('tour.next')}
          </button>
        </div>
      </div>
    </div>
  )
}
