import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { errorText } from '@/lib/errors'

/**
 * Something actually failed — as opposed to returning nothing. The shared
 * error Notice (role="alert"), with the retry as its action.
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

  return (
    <div data-testid="error-state">
      <Notice
        variant="error"
        title={t('error.title')}
        action={
          onRetry && (
            <button
              type="button"
              data-testid="error-retry"
              onClick={onRetry}
              className="inline-flex w-fit items-center px-3.5 font-medium"
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
          )
        }
      >
        <p style={{ fontVariantNumeric: 'tabular-nums', textWrap: 'pretty' }}>{errorText(error, t)}</p>
      </Notice>
    </div>
  )
}
