import { useTranslation } from 'react-i18next'

import { ErrorState } from '@/components/ErrorState'
import { Loading } from '@/components/controls'
import { SectionTitle } from '@/features/ops/surveys/Field'
import { countAndAmount, formatFigure, UNKNOWN } from '@/features/ops/surveys/figures'
import { isChoiceKind, parseOptions } from '@/features/ops/surveys/questionOptions'
import {
  useSurveyResults,
  type NumberSummaryRow,
  type SurveyQuestion,
  type TallyRow,
} from '@/features/ops/surveys/useSurveyAdmin'
import { localisedField } from '@/lib/names'

/**
 * Results: GET .../results — the API's counts and cash sums, then each
 * question's answers as it tallied them.
 *
 * Nothing is counted here. The one thing the screen supplies is the label an
 * option value stands for, which lives on the question.
 */
export function SurveyResults({
  surveyId,
  decimals,
  questions,
}: {
  surveyId: string
  /** The survey reward's: results carry the currency code but not its decimals. */
  decimals: number
  questions: SurveyQuestion[]
}) {
  const { t, i18n } = useTranslation()
  const tally = useSurveyResults(surveyId)
  const summary = tally.data
  const money = (figure: Parameters<typeof countAndAmount>[0]) =>
    countAndAmount(figure, summary?.currency ?? '', decimals, i18n.language)

  return (
    <section className="flex flex-col gap-4" data-testid="survey-results">
      <SectionTitle>{t('surveyAdmin.results')}</SectionTitle>

      <dl data-testid="survey-summary" className="grid grid-cols-2 gap-2.5 md:grid-cols-3 xl:grid-cols-6">
        <Figure label={t('surveyAdmin.summary.responses')} value={summary?.responses ?? UNKNOWN} testId="summary-responses" />
        <Figure
          label={t('surveyAdmin.summary.issued')}
          value={money(summary?.issued)}
          testId="summary-issued"
        />
        <Figure
          label={t('surveyAdmin.summary.redeemed')}
          value={money(summary?.redeemed)}
          testId="summary-redeemed"
        />
        <Figure
          label={t('surveyAdmin.summary.outstanding')}
          value={money(summary?.outstanding)}
          testId="summary-outstanding"
        />
        <Figure label={t('surveyAdmin.summary.expired')} value={summary?.expired_count ?? UNKNOWN} testId="summary-expired" />
        <Figure label={t('surveyAdmin.summary.void')} value={summary?.void_count ?? UNKNOWN} testId="summary-void" />
      </dl>

      <div className="flex flex-col gap-2.5" data-testid="survey-tally">
        <SectionTitle>{t('surveyAdmin.tally')}</SectionTitle>
        {tally.error ? (
          <ErrorState error={tally.error} onRetry={() => void tally.refetch()} />
        ) : tally.isLoading ? (
          <Loading testId="tally-loading" />
        ) : (
          <ol className="flex flex-col gap-2.5">
            {questions.map((question, index) => (
              <QuestionTally
                key={question.id}
                number={index + 1}
                question={question}
                choices={(tally.data?.tally ?? []).filter((row) => row.question_id === question.id)}
                numbers={(tally.data?.numbers ?? []).find((row) => row.question_id === question.id)}
              />
            ))}
          </ol>
        )}
      </div>
    </section>
  )
}

function Figure({ label, value, testId }: { label: string; value: string | number; testId: string }) {
  return (
    <div
      className="flex flex-col gap-1 px-3.5 py-3"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
    >
      <dt className="type-note" style={{ color: 'var(--ink-2)' }}>
        {label}
      </dt>
      <dd data-testid={testId} className="type-body-strong tabular">
        {value}
      </dd>
    </div>
  )
}

function QuestionTally({
  number,
  question,
  choices,
  numbers,
}: {
  number: number
  question: SurveyQuestion
  choices: TallyRow[]
  numbers: NumberSummaryRow | undefined
}) {
  const { t, i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  return (
    <li
      data-testid="tally-question"
      className="flex flex-col gap-2 px-4 py-3"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
    >
      <div className="flex flex-col gap-0.5">
        <span className="type-body-strong">
          {number}. {localisedField(question, 'prompt', language)}
        </span>
        <span className="type-note" style={{ color: 'var(--ink-3)' }}>
          {t(`surveyAdmin.kinds.${question.kind}`)}
        </span>
      </div>

      {(isChoiceKind(question.kind) || question.kind === 'yes_no') && (
        <dl className="flex flex-col">
          {choiceRows(question, choices, t, language).map((row) => (
            <div
              key={row.key}
              data-testid="tally-option"
              className="grid items-baseline gap-3 py-1"
              style={{ gridTemplateColumns: 'minmax(0, 1fr) auto', borderTop: '1px solid var(--rule)' }}
            >
              <dt className="type-small" style={{ color: 'var(--ink)' }}>
                {row.label}
              </dt>
              <dd className="type-body-strong tabular">{row.count}</dd>
            </div>
          ))}
        </dl>
      )}

      {question.kind === 'number' && (
        <p className="type-small flex flex-wrap gap-x-4 gap-y-1 tabular" style={{ color: 'var(--ink-2)' }}>
          <span data-testid="tally-average" className="font-semibold" style={{ color: 'var(--ink)' }}>
            {t('surveyAdmin.average', { value: formatFigure(numbers?.average) })}
          </span>
          <span data-testid="tally-minimum">{t('surveyAdmin.lowest', { value: formatFigure(numbers?.minimum) })}</span>
          <span data-testid="tally-maximum">{t('surveyAdmin.highest', { value: formatFigure(numbers?.maximum) })}</span>
          <span data-testid="tally-count">{t('surveyAdmin.answerCount', { count: numbers?.answers ?? 0 })}</span>
        </p>
      )}

      {question.kind === 'text' && (
        <p data-testid="tally-text" className="type-note" style={{ color: 'var(--ink-3)' }}>
          {t('surveyAdmin.textNotTallied')}
        </p>
      )}
    </li>
  )
}

interface ChoiceRow {
  key: string
  label: string
  count: number
}

/**
 * Labels for the tally's option values.
 *
 * Every listed option gets a row, in the question's order. The API groups
 * answers by value, so an option nobody chose has no row there — its count is
 * zero, which is an answer. A value the question no longer lists still gets a
 * row under its raw value rather than vanishing from the results.
 */
function choiceRows(
  question: SurveyQuestion,
  choices: TallyRow[],
  t: (key: string) => string,
  language: string | undefined,
): ChoiceRow[] {
  const countOf = (value: string) => choices.find((row) => row.option_value === value)?.answers ?? 0
  const listed: ChoiceRow[] =
    question.kind === 'yes_no'
      ? [
          { key: 'true', label: t('common.yes'), count: countOf('true') },
          { key: 'false', label: t('common.no'), count: countOf('false') },
        ]
      : parseOptions(question.options).map((option) => ({
          key: option.value,
          label: localisedField(option, 'label', language),
          count: countOf(option.value),
        }))
  const known = new Set(listed.map((row) => row.key))
  const unlisted = choices
    .filter((row) => !known.has(row.option_value))
    .map((row) => ({ key: row.option_value, label: row.option_value, count: row.answers }))
  return [...listed, ...unlisted]
}
