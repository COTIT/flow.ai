import { useId, useState } from 'react'
import { ArrowDown, ArrowUp, Plus } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ErrorState } from '@/components/ErrorState'
import { CONTROL } from '@/components/controlStyles'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { Field, Panel, SectionTitle } from '@/features/ops/surveys/Field'
import {
  QUESTION_KINDS,
  isChoiceKind,
  optionsForSave,
  parseOptions,
  type EditableOption,
  type QuestionKind,
} from '@/features/ops/surveys/questionOptions'
import {
  useRemoveQuestion,
  useSaveQuestion,
  useSwapQuestions,
  type SurveyQuestion,
} from '@/features/ops/surveys/useSurveyAdmin'
import { localisedField } from '@/lib/names'

/**
 * The questions of a draft survey, for an admin.
 *
 * Every rule about what a publishable question looks like — at least one
 * question, two options per choice question, a value and an English label per
 * option — is survey_guard's, checked when the survey goes live. The editor
 * sends what was typed; the guard's sentence is the feedback.
 */
export function QuestionsEditor({ surveyId, questions }: { surveyId: string; questions: SurveyQuestion[] }) {
  const { t } = useTranslation()
  const remove = useRemoveQuestion(surveyId)
  const swap = useSwapQuestions(surveyId)
  const [adding, setAdding] = useState(false)
  const nextPosition = (questions.at(-1)?.position ?? 0) + 1
  const listError = remove.isError ? remove.error : swap.isError ? swap.error : null

  return (
    <section className="flex flex-col gap-3" data-testid="questions-editor">
      <div className="flex flex-col gap-1">
        <SectionTitle>{t('surveyAdmin.questions')}</SectionTitle>
        <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {t('surveyAdmin.swahiliNote')}
        </p>
      </div>

      {questions.length === 0 && !adding && (
        <p className="type-body" style={{ color: 'var(--ink-2)' }}>
          {t('surveyAdmin.noQuestions')}
        </p>
      )}

      {listError && (
        <div data-testid="questions-error">
          <ErrorState error={listError} />
        </div>
      )}

      <ol className="flex flex-col gap-3">
        {questions.map((question, index) => (
          <li key={question.id}>
            <QuestionCard
              surveyId={surveyId}
              question={question}
              number={index + 1}
              position={question.position}
              canMoveUp={index > 0 && !swap.isPending}
              canMoveDown={index < questions.length - 1 && !swap.isPending}
              onMoveUp={() =>
                swap.mutate({
                  first: { id: question.id, position: question.position },
                  second: { id: questions[index - 1].id, position: questions[index - 1].position },
                })
              }
              onMoveDown={() =>
                swap.mutate({
                  first: { id: question.id, position: question.position },
                  second: { id: questions[index + 1].id, position: questions[index + 1].position },
                })
              }
              onRemove={() => remove.mutate(question.id)}
              removing={remove.isPending}
            />
          </li>
        ))}
        {adding && (
          <li>
            <QuestionCard
              surveyId={surveyId}
              number={questions.length + 1}
              position={nextPosition}
              onRemove={() => setAdding(false)}
              onAdded={() => setAdding(false)}
            />
          </li>
        )}
      </ol>

      {!adding && (
        <Button
          type="button"
          variant="secondary"
          data-testid="question-add"
          className="w-fit"
          onClick={() => setAdding(true)}
        >
          <Plus aria-hidden size={16} />
          {t('surveyAdmin.addQuestion')}
        </Button>
      )}
    </section>
  )
}

interface CardState {
  prompt_en: string
  prompt_sw: string
  kind: QuestionKind
  required: boolean
  options: EditableOption[]
}

const blankState = (): CardState => ({
  prompt_en: '',
  prompt_sw: '',
  kind: 'single_choice',
  required: true,
  options: [],
})

function stateOf(question: SurveyQuestion): CardState {
  return {
    prompt_en: question.prompt_en,
    prompt_sw: question.prompt_sw ?? '',
    kind: question.kind,
    required: question.required,
    options: parseOptions(question.options),
  }
}

/**
 * One question. Held in component state rather than a persisted draft: the
 * options are a list, and a persisted form holds strings.
 */
function QuestionCard({
  surveyId,
  question,
  number,
  position,
  canMoveUp = false,
  canMoveDown = false,
  onMoveUp,
  onMoveDown,
  onRemove,
  onAdded,
  removing = false,
}: {
  surveyId: string
  question?: SurveyQuestion
  number: number
  position: number
  canMoveUp?: boolean
  canMoveDown?: boolean
  onMoveUp?: () => void
  onMoveDown?: () => void
  onRemove: () => void
  onAdded?: () => void
  removing?: boolean
}) {
  const { t } = useTranslation()
  const id = useId()
  const save = useSaveQuestion(surveyId)
  const [state, setState] = useState<CardState>(() => (question ? stateOf(question) : blankState()))
  const [dirty, setDirty] = useState(false)

  const change = (patch: Partial<CardState>) => {
    setState((s) => ({ ...s, ...patch }))
    setDirty(true)
  }
  const changeOption = (index: number, patch: Partial<EditableOption>) =>
    change({ options: state.options.map((o, i) => (i === index ? { ...o, ...patch } : o)) })

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (save.isPending) return
    const options = optionsForSave(state.kind, state.options)
    save
      .mutateAsync({
        ...(question ? { id: question.id } : {}),
        position,
        kind: state.kind,
        prompt_en: state.prompt_en,
        prompt_sw: state.prompt_sw,
        required: state.required,
        options,
      })
      .then(() => {
        if (!question) {
          onAdded?.()
          return
        }
        // The values made on this save are now the options' identity: hold
        // them, so a later label edit cannot mint a different one.
        setState((s) => ({ ...s, options: parseOptions(options) }))
        setDirty(false)
      })
      // The mutation's own error state shows the refusal.
      .catch(() => undefined)
  }

  return (
    <Panel testId={question ? 'question-card' : 'question-card-new'}>
      <form className="flex flex-col gap-3" noValidate onSubmit={submit}>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="type-body-strong tabular">{number}.</span>
          <div className="flex flex-wrap gap-1.5">
            {question && (
              <>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-testid="question-move-up"
                  disabled={!canMoveUp}
                  onClick={onMoveUp}
                  aria-label={t('surveyAdmin.question.moveUp')}
                >
                  <ArrowUp aria-hidden size={16} />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-testid="question-move-down"
                  disabled={!canMoveDown}
                  onClick={onMoveDown}
                  aria-label={t('surveyAdmin.question.moveDown')}
                >
                  <ArrowDown aria-hidden size={16} />
                </Button>
              </>
            )}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-testid="question-remove"
              disabled={removing}
              onClick={onRemove}
            >
              {t('surveyAdmin.question.remove')}
            </Button>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <Field label={t('surveyAdmin.question.prompt_en')} htmlFor={`${id}-prompt-en`}>
            <input
              id={`${id}-prompt-en`}
              data-testid="question-prompt-en"
              value={state.prompt_en}
              onChange={(e) => change({ prompt_en: e.target.value })}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.question.prompt_sw')} htmlFor={`${id}-prompt-sw`}>
            <input
              id={`${id}-prompt-sw`}
              data-testid="question-prompt-sw"
              lang="sw"
              value={state.prompt_sw}
              onChange={(e) => change({ prompt_sw: e.target.value })}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.question.kind')} htmlFor={`${id}-kind`}>
            <Select value={state.kind} onValueChange={(value) => change({ kind: value as QuestionKind })}>
              <SelectTrigger id={`${id}-kind`} data-testid="question-kind" className="w-full">
                {t(`surveyAdmin.kinds.${state.kind}`)}
              </SelectTrigger>
              <SelectContent>
                {QUESTION_KINDS.map((kind) => (
                  <SelectItem key={kind} value={kind}>
                    {t(`surveyAdmin.kinds.${kind}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </Field>
          <label
            className="flex items-center gap-2 self-end"
            style={{ minHeight: 40, fontSize: 14, color: 'var(--ink)' }}
          >
            <input
              type="checkbox"
              data-testid="question-required"
              checked={state.required}
              onChange={(e) => change({ required: e.target.checked })}
              style={{ width: 18, height: 18, accentColor: 'var(--primary)' }}
            />
            {t('surveyAdmin.question.required')}
          </label>
        </div>

        {isChoiceKind(state.kind) && (
          <fieldset className="flex flex-col gap-2">
            <legend className="type-note mb-1.5 font-semibold" style={{ color: 'var(--ink-2)' }}>
              {t('surveyAdmin.question.options')}
            </legend>
            {state.options.map((option, index) => (
              <div
                key={index}
                data-testid="question-option"
                className="grid grid-cols-1 items-center gap-2 md:grid-cols-[1fr_1fr_auto]"
              >
                <input
                  data-testid="question-option-label-en"
                  aria-label={t('surveyAdmin.question.optionLabel')}
                  placeholder={t('surveyAdmin.question.optionLabel')}
                  value={option.label_en}
                  onChange={(e) => changeOption(index, { label_en: e.target.value })}
                  className="w-full"
                  style={CONTROL}
                />
                <input
                  data-testid="question-option-label-sw"
                  aria-label={t('surveyAdmin.question.optionLabelSw')}
                  placeholder={t('surveyAdmin.question.optionLabelSw')}
                  lang="sw"
                  value={option.label_sw}
                  onChange={(e) => changeOption(index, { label_sw: e.target.value })}
                  className="w-full"
                  style={CONTROL}
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  data-testid="question-option-remove"
                  onClick={() => change({ options: state.options.filter((_, i) => i !== index) })}
                >
                  {t('surveyAdmin.question.remove')}
                </Button>
              </div>
            ))}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              data-testid="question-add-option"
              className="w-fit"
              onClick={() => change({ options: [...state.options, { value: '', label_en: '', label_sw: '' }] })}
            >
              <Plus aria-hidden size={14} />
              {t('surveyAdmin.question.addOption')}
            </Button>
          </fieldset>
        )}

        {save.isError && <ErrorState error={save.error} />}

        <div className="flex flex-wrap items-center gap-3">
          <Button type="submit" data-testid="question-save" disabled={save.isPending} className="w-fit">
            {save.isPending ? t('surveyAdmin.saving') : t('surveyAdmin.save')}
          </Button>
          {question && save.isSuccess && !dirty && (
            <span role="status" className="type-note" style={{ color: 'var(--green-ink)' }}>
              {t('surveyAdmin.saved')}
            </span>
          )}
        </div>
      </form>
    </Panel>
  )
}

/** The questions as farmers will read them, with no controls. */
export function QuestionList({ questions }: { questions: SurveyQuestion[] }) {
  const { t, i18n } = useTranslation()
  const language = i18n.resolvedLanguage

  return (
    <section className="flex flex-col gap-3">
      <SectionTitle>{t('surveyAdmin.questions')}</SectionTitle>
      {questions.length === 0 ? (
        <p className="type-body" style={{ color: 'var(--ink-2)' }}>
          {t('surveyAdmin.noQuestions')}
        </p>
      ) : (
        <ol data-testid="question-list" className="flex flex-col gap-2">
          {questions.map((question, index) => {
            const options = parseOptions(question.options)
            return (
              <li
                key={question.id}
                className="flex flex-col gap-1 px-4 py-3"
                style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
              >
                <span className="type-body-strong">
                  {index + 1}. {localisedField(question, 'prompt', language)}
                </span>
                <span className="type-note" style={{ color: 'var(--ink-2)' }}>
                  {t(`surveyAdmin.kinds.${question.kind}`)}
                  {question.required ? ` · ${t('surveyAdmin.question.required')}` : ''}
                </span>
                {isChoiceKind(question.kind) && options.length > 0 && (
                  <ul className="type-note flex flex-wrap gap-x-3" style={{ color: 'var(--ink-2)' }}>
                    {options.map((option) => (
                      <li key={option.value || option.label_en}>{localisedField(option, 'label', language)}</li>
                    ))}
                  </ul>
                )}
              </li>
            )
          })}
        </ol>
      )}
    </section>
  )
}
