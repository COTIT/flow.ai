import { useState, type CSSProperties } from 'react'
import { Link } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { BUTTON_PRIMARY, BUTTON_SECONDARY } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { StatusPill } from '@/components/StatusPill'
import {
  useSurveyList,
  type FarmerSurvey,
  type SurveyCardState,
  type SurveyListRow,
} from '@/features/farmer/useSurveys'
import { voucherDisplayStatus } from '@/features/farmer/useVouchers'
import { formatMoney, formatTimestamp } from '@/lib/format'
import { dbReasonText } from '@/lib/dbMessages'
import { localisedField } from '@/lib/names'

/**
 * `/farm/surveys` — every survey this household can see, and what it can do
 * with each.
 *
 * Nothing here decides eligibility. "New", "Not available" and the reason are
 * the database's answer from `app_survey_eligibility`, which shares
 * `survey_block_reason` with the submit — so the list can never offer a
 * survey the submit would refuse, nor hide one it would accept.
 */
export function SurveysListScreen() {
  const { t } = useTranslation()
  const query = useSurveyList()
  // Read once per mount; expiry is a date, not a countdown.
  const [now] = useState(() => Date.now())

  if (query.error) return <ErrorState error={query.error} onRetry={() => query.refetch()} />

  const rows = query.data ?? []

  return (
    <section data-testid="surveys-list" className="flex max-w-lg flex-col gap-4">
      <header className="flex flex-col gap-2">
        <h1 className="type-screen-title">{t('surveys.title')}</h1>
        <p className="type-body" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('surveys.intro')}
        </p>
      </header>

      {query.isLoading ? (
        <Loading testId="surveys-loading" />
      ) : rows.length === 0 ? (
        <EmptyState title={t('surveys.emptyTitle')} detail={t('surveys.emptyDetail')} />
      ) : (
        <ul className="flex flex-col gap-3">
          {rows.map((row) => (
            <SurveyCard key={row.survey.id} row={row} now={now} />
          ))}
        </ul>
      )}
    </section>
  )
}

function SurveyCard({ row, now }: { row: SurveyListRow; now: number }) {
  const { t, i18n } = useTranslation()
  const language = i18n.resolvedLanguage
  const { survey } = row
  const description = localisedField(survey, 'description', language)

  return (
    <li
      data-testid="survey-card"
      data-state={row.state}
      data-survey-id={survey.id}
      className="flex flex-col gap-3 p-4"
      style={{
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      <div className="flex flex-wrap items-start justify-between gap-2.5">
        <h2 className="min-w-0" style={{ fontSize: 17, lineHeight: '24px', fontWeight: 600, textWrap: 'pretty' }}>
          {localisedField(survey, 'title', language)}
        </h2>
        <StateTag state={row.state} />
      </div>

      {description && (
        <p className="type-small" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {description}
        </p>
      )}

      <SurveyIncentive amount={survey.reward_amount} currency={survey.currency} />
      <SurveyFacts survey={survey} />

      {row.state === 'new' && (
        <Link
          to="/farm/surveys/$surveyId"
          params={{ surveyId: survey.id }}
          data-testid="survey-open"
          className="inline-flex w-full items-center justify-center"
          style={{ ...BUTTON_PRIMARY, minHeight: 48, fontSize: 16 }}
        >
          {t('surveys.open')}
        </Link>
      )}

      {row.state === 'answered' && (
        <div className="flex flex-col gap-2.5">
          {row.voucher && (
            <div>
              <StatusPill kind="voucher" status={voucherDisplayStatus(row.voucher, now)} />
            </div>
          )}
          <Link
            to="/farm/surveys/$surveyId"
            params={{ surveyId: survey.id }}
            data-testid="survey-view-voucher"
            className="inline-flex w-full items-center justify-center"
            style={{ ...BUTTON_SECONDARY, minHeight: 48, fontSize: 16 }}
          >
            {t('surveys.view')}
          </Link>
        </div>
      )}

      {row.state === 'unavailable' && row.reason && (
        <p
          data-testid="survey-reason"
          className="type-small px-3 py-2.5"
          style={{
            border: '1px solid var(--rule)',
            borderRadius: 'var(--radius-control)',
            background: 'var(--sand-2)',
            color: 'var(--ink-2)',
            textWrap: 'pretty',
          }}
        >
          {dbReasonText(t, row.reason)}
        </p>
      )}
    </li>
  )
}

/**
 * The incentive: a fixed cash amount per household per survey, handed over at
 * the Ruaha office. Not earnings, not a balance — and not indicative: the
 * amount is fixed. One rendering, shared by the list and the survey screen,
 * so the sentence cannot drift between them.
 */
export function SurveyIncentive({ amount, currency }: { amount: number; currency: string }) {
  const { t } = useTranslation()
  return (
    <div
      data-testid="survey-incentive"
      className="flex flex-col gap-0.5 px-3.5 py-3"
      style={{
        border: '1px solid var(--primary)',
        borderRadius: 'var(--radius-control)',
        background: 'var(--primary-tint)',
        color: 'var(--primary-ink)',
      }}
    >
      <span className="type-note">{t('surveys.incentive')}</span>
      <span className="type-figure tabular">{formatMoney(amount, currency)}</span>
      <span className="type-small">{t('surveys.incentiveNote')}</span>
    </div>
  )
}

/** Question count, and the closing date when there is one. */
export function SurveyFacts({ survey }: { survey: Pick<FarmerSurvey, 'questions' | 'closes_at'> }) {
  const { t } = useTranslation()
  return (
    <p className="type-note" style={{ color: 'var(--ink-3)' }}>
      {[
        t('surveys.questionCount', { count: survey.questions.length }),
        survey.closes_at && t('surveys.closes', { date: formatTimestamp(survey.closes_at) }),
      ]
        .filter(Boolean)
        .join(' · ')}
    </p>
  )
}

const TAG_STYLE: Record<SurveyCardState, CSSProperties> = {
  new: { border: '1px solid var(--primary)', background: 'var(--primary-tint)', color: 'var(--primary-ink)' },
  answered: { border: '1px solid var(--green-ink)', background: 'var(--green-tint)', color: 'var(--green-ink)' },
  unavailable: { border: '1px solid var(--rule-2)', background: 'var(--sand-2)', color: 'var(--ink-2)' },
}

const TAG_KEY: Record<SurveyCardState, string> = {
  new: 'surveys.new',
  answered: 'surveys.answered',
  unavailable: 'surveys.notAvailable',
}

function StateTag({ state }: { state: SurveyCardState }) {
  const { t } = useTranslation()
  return (
    <span
      data-testid="survey-state"
      className="type-note inline-flex items-center px-2.5 py-0.5"
      style={{ ...TAG_STYLE[state], borderRadius: 'var(--radius-pill)', fontWeight: 600, flex: 'none' }}
    >
      {t(TAG_KEY[state])}
    </span>
  )
}
