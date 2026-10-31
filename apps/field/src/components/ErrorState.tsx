import { useTranslation } from 'react-i18next'

import { humanizeDbError } from '@/lib/errors'

/**
 * Something actually failed — as opposed to returning nothing.
 *
 * Error contract §9: the Postgres messages in this schema are written to be
 * read by humans, and they are surfaced VERBATIM —
 * `over-commitment: 4100.00 kg available, ...` names the numbers the user
 * needs, and no rewriting improves it. Which is also why the message is set to
 * wrap and never to clip: that sentence runs to two lines on a phone, and every
 * figure in it is load-bearing.
 *
 * `humanizeDbError` is what decides. Its default IS verbatim; it replaces only
 * what it recognises as machine noise — a constraint identifier, a uuid parse
 * failure, an overflow, a JS exception. Those were reaching users as copy
 * (QA #4, #20, #25), which is not what §9 asks for and never was.
 */
export function ErrorState({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  const { t } = useTranslation()
  const human = humanizeDbError(error)
  const message = human.kind === 'verbatim' ? human.message : t(human.key, human.values)

  return (
    <div
      data-testid="error-state"
      role="alert"
      className="flex flex-col gap-1.5 px-4 py-3.5"
      style={{
        background: 'var(--flag-tint)',
        border: '1px solid rgba(158, 27, 27, .25)',
        borderLeft: '4px solid var(--flag-ink)',
        borderRadius: 'var(--radius-card)',
      }}
    >
      <p className="flex items-center gap-2" style={{ color: 'var(--flag-ink)' }}>
        <span
          data-mark="error"
          aria-hidden
          className="inline-flex items-center justify-center"
          style={{
            width: 18,
            height: 18,
            borderRadius: 'var(--radius-pill)',
            background: 'var(--flag-ink)',
            color: '#fff',
            fontSize: 12,
            fontWeight: 700,
            lineHeight: 1,
            flex: 'none',
          }}
        >
          !
        </span>
        <span style={{ fontSize: 15, lineHeight: '22px', fontWeight: 600 }}>{t('error.title')}</span>
      </p>
      <p
        style={{
          fontSize: 14,
          lineHeight: 1.55,
          color: 'var(--ink)',
          fontVariantNumeric: 'tabular-nums',
          textWrap: 'pretty',
        }}
      >
        {message}
      </p>
      {onRetry && (
        <button
          type="button"
          data-testid="error-retry"
          onClick={onRetry}
          className="mt-1.5 inline-flex w-fit items-center px-3.5 font-medium"
          style={{
            minHeight: 40,
            border: '1px solid var(--rule-2)',
            borderRadius: 'var(--radius-control)',
            background: 'var(--paper)',
            color: 'var(--ink)',
            fontSize: 14,
          }}
        >
          {t('error.retry')}
        </button>
      )}
    </div>
  )
}
