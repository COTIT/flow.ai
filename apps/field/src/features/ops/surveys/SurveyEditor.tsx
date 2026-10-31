import { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { ErrorState } from '@/components/ErrorState'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { CONTROL } from '@/components/controlStyles'
import { Button } from '@/components/ui/button'
import { Field, Panel } from '@/features/ops/surveys/Field'
import { surveyFormValues, surveyPatch, type SurveyFormValues } from '@/features/ops/surveys/surveyForm'
import type { Survey, useUpdateSurvey } from '@/features/ops/surveys/useSurveyAdmin'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { usePersistentForm } from '@/lib/usePersistentForm'

/**
 * The survey's own fields, while it is a draft and the viewer is an admin.
 *
 * The mutation is owned by the screen and passed in: the editor is remounted
 * when the saved survey comes back (its `updated_at` is the draft version),
 * and a confirmed save should still say so after that.
 */
export function SurveyEditor({
  survey,
  update,
}: {
  survey: Survey
  update: ReturnType<typeof useUpdateSurvey>
}) {
  const { t } = useTranslation()
  const draft = usePersistentForm<SurveyFormValues>('survey-edit', survey.id, surveyFormValues(survey), undefined, {
    version: survey.updated_at,
  })
  const [titleEn, setTitleEn] = draft.field('title_en')
  const [titleSw, setTitleSw] = draft.field('title_sw')
  const [descriptionEn, setDescriptionEn] = draft.field('description_en')
  const [descriptionSw, setDescriptionSw] = draft.field('description_sw')
  const [reward, setReward] = draft.field('reward_amount')
  const [maxHouseholds, setMaxHouseholds] = draft.field('max_households')
  const [auditRate, setAuditRate] = draft.field('audit_rate')
  const [closesOn, setClosesOn] = draft.field('closes_on')
  const inFlight = useRef(false)

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (inFlight.current || update.isPending || !draft.ready) return
    inFlight.current = true
    void finishDraftWhenSaved(update.mutateAsync(surveyPatch(draft.values)), draft.finish).finally(() => {
      inFlight.current = false
    })
  }

  return (
    <Panel testId="survey-editor">
      <form className="flex flex-col gap-3" noValidate onSubmit={submit}>
        <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <Field label={t('surveyAdmin.fields.title_en')} htmlFor="survey-title-en">
            <input
              id="survey-title-en"
              data-testid="survey-title-en"
              value={titleEn}
              onChange={(e) => setTitleEn(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.title_sw')} htmlFor="survey-title-sw" hint={t('surveyAdmin.swahiliNote')}>
            <input
              id="survey-title-sw"
              data-testid="survey-title-sw"
              lang="sw"
              value={titleSw}
              onChange={(e) => setTitleSw(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.description_en')} htmlFor="survey-description-en">
            <textarea
              id="survey-description-en"
              data-testid="survey-description-en"
              rows={3}
              value={descriptionEn}
              onChange={(e) => setDescriptionEn(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.description_sw')} htmlFor="survey-description-sw">
            <textarea
              id="survey-description-sw"
              data-testid="survey-description-sw"
              lang="sw"
              rows={3}
              value={descriptionSw}
              onChange={(e) => setDescriptionSw(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.reward_amount')} htmlFor="survey-reward">
            <input
              id="survey-reward"
              data-testid="survey-reward"
              type="number"
              inputMode="decimal"
              min={0}
              value={reward}
              onChange={(e) => setReward(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.max_households')} htmlFor="survey-max-households">
            <input
              id="survey-max-households"
              data-testid="survey-max-households"
              type="number"
              inputMode="numeric"
              min={1}
              step={1}
              value={maxHouseholds}
              onChange={(e) => setMaxHouseholds(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.audit_rate')} htmlFor="survey-audit-rate" hint={t('surveyAdmin.auditNote')}>
            <input
              id="survey-audit-rate"
              data-testid="survey-audit-rate"
              type="number"
              inputMode="numeric"
              min={0}
              max={100}
              step={1}
              value={auditRate}
              onChange={(e) => setAuditRate(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.closes_at')} htmlFor="survey-closes-on">
            <input
              id="survey-closes-on"
              data-testid="survey-closes-on"
              type="date"
              value={closesOn}
              onChange={(e) => setClosesOn(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
        </div>

        {update.isError && <ErrorState error={update.error} />}

        <div className="flex flex-wrap items-center gap-3">
          <Button type="submit" data-testid="survey-save" disabled={!draft.ready || update.isPending} className="w-fit">
            {update.isPending ? t('surveyAdmin.saving') : t('surveyAdmin.save')}
          </Button>
          {update.isSuccess && !draft.dirty && (
            <span data-testid="survey-saved" role="status" className="type-note" style={{ color: 'var(--green-ink)' }}>
              {t('surveyAdmin.saved')}
            </span>
          )}
        </div>
      </form>
    </Panel>
  )
}
