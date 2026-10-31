import { useState, type CSSProperties, type ReactNode } from 'react'
import { Link, getRouteApi } from '@tanstack/react-router'
import { ArrowLeft } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { CONTROL_FIELD } from '@/components/controlStyles'
import { Loading, ProductNote } from '@/components/controls'
import { Button } from '@/components/ui/button'
import { SurveyFacts, SurveyIncentive } from '@/features/farmer/SurveysListScreen'
import { VoucherCard } from '@/features/farmer/VoucherCard'
import { useSurveyEligibility } from '@/features/farmer/useSurveyEligibility'
import {
  readChoices,
  sentence,
  toSurveyAnswers,
  useSubmitSurvey,
  useSurvey,
  writeChoices,
  type FarmerSurvey,
  type SurveyQuestion,
} from '@/features/farmer/useSurveys'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { humanizeDbError } from '@/lib/errors'
import { dbReasonText } from '@/lib/dbMessages'
import { localisedField } from '@/lib/names'
import { usePersistentForm } from '@/lib/usePersistentForm'

const route = getRouteApi('/_farmer/farm/surveys/$surveyId')

/**
 * `/farm/surveys/$surveyId` — one survey, in whichever state the database
 * says this household is in:
 *
 *   answered      the voucher it earned the household
 *   eligible      the answer form
 *   neither       the database's reason, and no form
 *
 * Nothing here re-derives eligibility, required answers or option validity.
 * The submit RPC runs the same `survey_block_reason` the list does, checks
 * every answer, and refuses in a sentence that names the question — which is
 * shown as written.
 */
export function SurveyScreen() {
  const { surveyId } = route.useParams()
  const { t, i18n } = useTranslation()
  const language = i18n.resolvedLanguage
  const eligibility = useSurveyEligibility()
  const query = useSurvey(surveyId)
  const submit = useSubmitSurvey(surveyId)

  const error = query.error ?? eligibility.error
  if (error) {
    return (
      <ErrorState
        error={error}
        onRetry={() => {
          void query.refetch()
          void eligibility.refetch()
        }}
      />
    )
  }
  if (query.isLoading || eligibility.isLoading) return <Loading testId="survey-loading" />

  const survey = query.data
  const row = eligibility.data?.find((r) => r.survey_id === surveyId)

  // Zero rows is an answer: not visible to this household, or not a survey.
  if (!survey || !row) {
    return (
      <Shell>
        <EmptyState title={t('surveys.notFound')} />
      </Shell>
    )
  }

  // A voucher from the submit that just returned wins over an eligibility
  // row that has not been refetched yet.
  const voucherId = submit.data?.voucher_id ?? row.voucher_id
  const description = localisedField(survey, 'description', language)

  return (
    <Shell>
      <header className="flex flex-col gap-2.5">
        <h1 className="type-screen-title" style={{ textWrap: 'balance' }}>
          {localisedField(survey, 'title', language)}
        </h1>
        {description && (
          <p className="type-body" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
            {description}
          </p>
        )}
        <SurveyIncentive amount={survey.reward_amount} currency={survey.currency} />
        <SurveyFacts survey={survey} />
      </header>

      {voucherId ? (
        <VoucherCard voucherId={voucherId} />
      ) : row.eligible ? (
        <SurveyAnswerForm survey={survey} submit={submit} />
      ) : row.response_id ? (
        <p data-testid="survey-answered" className="type-body-strong">
          {t('surveys.answered')}
        </p>
      ) : (
        <p
          data-testid="survey-blocked"
          className="type-body-strong px-3.5 py-3"
          style={{
            border: '1px solid var(--rule-2)',
            borderRadius: 'var(--radius-card)',
            background: 'var(--sand-2)',
            color: 'var(--ink)',
            textWrap: 'pretty',
          }}
        >
          {row.reason ? dbReasonText(t, row.reason) : t('surveys.notAvailable')}
        </p>
      )}
    </Shell>
  )
}

function Shell({ children }: { children: ReactNode }) {
  const { t } = useTranslation()
  return (
    <section data-testid="survey-screen" className="flex max-w-lg flex-col gap-4">
      <Link
        to="/farm/surveys"
        data-testid="survey-back"
        className="inline-flex min-h-11 items-center gap-2 self-start font-semibold"
        style={{ color: 'var(--primary-ink)' }}
      >
        <ArrowLeft aria-hidden size={16} strokeWidth={2.25} />
        {t('surveys.title')}
      </Link>
      {children}
    </section>
  )
}

type SubmitMutation = ReturnType<typeof useSubmitSurvey>

/**
 * The answers, as a draft that survives a reload (business-rules §12).
 *
 * Mounted only once the questions are known, so the draft's shape — one
 * string per question id — is fixed from its first render. The draft's
 * `clientRef` is the RPC's `p_client_ref`: a submit retried after a timeout,
 * or after a reload, replays the first save's voucher instead of being
 * refused as a second answer.
 */
function SurveyAnswerForm({ survey, submit }: { survey: FarmerSurvey; submit: SubmitMutation }) {
  const { t, i18n } = useTranslation()
  const language = i18n.resolvedLanguage
  const [defaults] = useState(() =>
    Object.fromEntries(survey.questions.map((question) => [question.id, ''])),
  )
  const draft = usePersistentForm<Record<string, string>>('survey-answers', survey.id, defaults)
  const {
    handleSubmit,
    formState: { isSubmitting },
  } = draft

  /**
   * The promise is returned, not voided, so react-hook-form's `isSubmitting`
   * holds for the whole round trip and a second tap cannot answer twice. The
   * draft clears only once the database has confirmed the save; any other
   * outcome keeps it, and shows through the mutation's error.
   */
  const onSubmit = handleSubmit((values) => {
    if (!draft.ready) return
    return finishDraftWhenSaved(
      submit.mutateAsync({
        answers: toSurveyAnswers(survey.questions, values),
        clientRef: draft.clientRef,
      }),
      draft.finish,
    )
  })

  const busy = submit.isPending || isSubmitting

  return (
    <form data-testid="survey-form" className="flex flex-col gap-5" noValidate onSubmit={onSubmit}>
      <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />

      {survey.questions.map((question) => {
        const [value, setValue] = draft.field(question.id)
        return (
          <QuestionField
            key={question.id}
            question={question}
            value={value ?? ''}
            onChange={setValue}
            language={language}
          />
        )
      })}

      <ProductNote>{t('surveys.onceNote')}</ProductNote>

      {submit.error && <SubmitRefusal error={submit.error} />}

      <Button
        type="submit"
        variant="primary"
        size="lg"
        data-testid="survey-submit"
        className="w-full font-semibold"
        disabled={!draft.ready || busy}
      >
        {busy ? t('surveys.submitting') : t('surveys.submit')}
      </Button>
    </form>
  )
}

/**
 * The database's refusal, as written — "question 3 is required", "this survey
 * is closed" — with its first letter raised. Machine noise (a dropped
 * connection, a constraint name) is replaced by `humanizeDbError`, which is
 * what `ErrorState` uses too; the refusal is not dressed as "something went
 * wrong", because nothing did: the database said no, and why.
 */
function SubmitRefusal({ error }: { error: Error }) {
  const { t } = useTranslation()
  const human = humanizeDbError(error)
  return (
    <p
      role="alert"
      data-testid="survey-submit-error"
      className="type-body px-3.5 py-3"
      style={{
        border: '1px solid var(--rule-2)',
        borderLeft: '4px solid var(--flag-ink)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--flag-tint)',
        color: 'var(--ink)',
        textWrap: 'pretty',
      }}
    >
      {human.kind === 'verbatim' ? sentence(human.message) : t(human.key, human.values)}
    </p>
  )
}

interface FieldProps {
  question: SurveyQuestion
  value: string
  onChange: (value: string) => void
  language: string | undefined
}

/**
 * One question. The position is shown because the database's refusals name
 * it ("question 3 is required"), and the farmer has to find question 3.
 */
function QuestionField({ question, value, onChange, language }: FieldProps) {
  const { t } = useTranslation()
  const inputId = `survey-q-${question.id}`
  const prompt = (
    <span className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
      <span className="type-body-strong" style={{ color: 'var(--ink)', textWrap: 'pretty' }}>
        {`${question.position}. ${localisedField(question, 'prompt', language)}`}
      </span>
      <span className="type-note" style={{ color: question.required ? 'var(--ink-2)' : 'var(--ink-3)' }}>
        {question.required ? t('surveys.required') : t('surveys.optional')}
      </span>
    </span>
  )

  switch (question.kind) {
    case 'single_choice':
    case 'yes_no': {
      const options =
        question.kind === 'yes_no'
          ? [
              { value: 'yes', label: t('surveys.yes') },
              { value: 'no', label: t('surveys.no') },
            ]
          : question.options.map((option) => ({
              value: option.value,
              label: localisedField(option, 'label', language),
            }))
      return (
        <fieldset data-testid="survey-question" data-kind={question.kind} className="flex flex-col gap-2">
          <legend className="mb-2">{prompt}</legend>
          {options.map((option) => (
            <Choice
              key={option.value}
              type="radio"
              name={inputId}
              value={option.value}
              label={option.label}
              checked={value === option.value}
              onChange={() => onChange(option.value)}
            />
          ))}
        </fieldset>
      )
    }

    case 'multi_choice': {
      const chosen = readChoices(value)
      const toggle = (optionValue: string) => {
        const next = chosen.includes(optionValue)
          ? chosen.filter((v) => v !== optionValue)
          : [...chosen, optionValue]
        // Kept in the order the options are listed, whatever order they were ticked.
        onChange(writeChoices(question.options.map((o) => o.value).filter((v) => next.includes(v))))
      }
      return (
        <fieldset data-testid="survey-question" data-kind={question.kind} className="flex flex-col gap-2">
          <legend className="mb-2">{prompt}</legend>
          {question.options.map((option) => (
            <Choice
              key={option.value}
              type="checkbox"
              name={inputId}
              value={option.value}
              label={localisedField(option, 'label', language)}
              checked={chosen.includes(option.value)}
              onChange={() => toggle(option.value)}
            />
          ))}
        </fieldset>
      )
    }

    case 'number':
      return (
        <div data-testid="survey-question" data-kind={question.kind} className="flex flex-col gap-2">
          <label htmlFor={inputId}>{prompt}</label>
          {/* Text with a decimal keypad, not type="number": the database parses
              the value and names the question when it cannot. */}
          <input
            id={inputId}
            type="text"
            inputMode="decimal"
            autoComplete="off"
            value={value}
            onChange={(event) => onChange(event.target.value)}
            style={{ ...CONTROL_FIELD, width: '100%', fontVariantNumeric: 'tabular-nums' }}
          />
        </div>
      )

    case 'text':
      return (
        <div data-testid="survey-question" data-kind={question.kind} className="flex flex-col gap-2">
          <label htmlFor={inputId}>{prompt}</label>
          <textarea
            id={inputId}
            rows={3}
            value={value}
            onChange={(event) => onChange(event.target.value)}
            style={{ ...CONTROL_FIELD, width: '100%' }}
          />
        </div>
      )
  }
}

/** A radio or checkbox row with a 48px target — tapped by a thumb, outdoors. */
function Choice({
  type,
  name,
  value,
  label,
  checked,
  onChange,
}: {
  type: 'radio' | 'checkbox'
  name: string
  value: string
  label: string
  checked: boolean
  onChange: () => void
}) {
  return (
    <label className="flex cursor-pointer items-center gap-3 px-3.5 py-2" style={choiceRow(checked)}>
      <input
        type={type}
        name={name}
        value={value}
        checked={checked}
        onChange={onChange}
        style={{ width: 20, height: 20, flex: 'none', accentColor: 'var(--primary)' }}
      />
      <span className="type-body" style={{ textWrap: 'pretty' }}>
        {label}
      </span>
    </label>
  )
}

function choiceRow(checked: boolean): CSSProperties {
  return {
    minHeight: 48,
    boxSizing: 'border-box',
    border: checked ? '1.5px solid var(--primary)' : '1.5px solid var(--rule-2)',
    borderRadius: 'var(--radius-control)',
    background: checked ? 'var(--primary-tint)' : 'var(--paper)',
    color: 'var(--ink)',
  }
}
