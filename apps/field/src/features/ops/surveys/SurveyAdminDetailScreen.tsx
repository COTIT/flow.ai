import { useState } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { useSession } from '@/app/session'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Loading } from '@/components/controls'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { Button } from '@/components/ui/button'
import { UNKNOWN } from '@/features/ops/surveys/figures'
import { canAuthorSurveys } from '@/features/ops/surveys/permissions'
import { QuestionList, QuestionsEditor } from '@/features/ops/surveys/QuestionsEditor'
import { SurveyEditor } from '@/features/ops/surveys/SurveyEditor'
import { SurveyResults } from '@/features/ops/surveys/SurveyResults'
import { SurveyVouchers } from '@/features/ops/surveys/SurveyVouchers'
import { timestampToDarDate } from '@/features/ops/surveys/surveyDates'
import {
  useSetSurveyStatus,
  useSurveyAdminDetail,
  useUpdateSurvey,
  type Survey,
} from '@/features/ops/surveys/useSurveyAdmin'
import { useActorName } from '@/lib/actorNames'
import { formatMoney, formatPercent, formatPlainDate, formatTimestamp } from '@/lib/format'
import { localisedField } from '@/lib/names'

const route = getRouteApi('/_ops/ops/surveys/$surveyId')

type StatusChange = 'live' | 'closed'

/**
 * One survey, for ops and admin.
 *
 *   draft, admin      the editor, the questions, Publish
 *   draft, ops        the survey as written, read-only
 *   live or closed    results and vouchers; an admin may close a live one
 *
 * The status machine (draft -> live -> closed) and every precondition of
 * going live belong to survey_guard. This screen offers the one legal next
 * step to an admin, and shows the guard's sentence if it refuses.
 */
export function SurveyAdminDetailScreen() {
  const { surveyId } = route.useParams()
  const { t, i18n } = useTranslation()
  const query = useSurveyAdminDetail(surveyId)
  const { data: session } = useSession()
  const admin = canAuthorSurveys(session?.memberships)
  const update = useUpdateSurvey(surveyId)
  const setStatus = useSetSurveyStatus(surveyId)
  const [confirming, setConfirming] = useState<StatusChange | null>(null)

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />
  if (query.isLoading) return <Loading testId="survey-detail-loading" />
  // Zero rows is an answer: this survey is not visible to this user.
  if (!query.data) return <EmptyState title={t('empty.noAccessToThis')} detail={t('empty.noAccessDetail')} />

  const { survey, questions, summary } = query.data
  const title = localisedField(survey, 'title', i18n.resolvedLanguage)
  const draft = survey.status === 'draft'
  const next: StatusChange | null = !admin ? null : draft ? 'live' : survey.status === 'live' ? 'closed' : null

  const change = (status: StatusChange) => {
    setStatus
      .mutateAsync(status)
      // The mutation's error state shows the guard's refusal.
      .catch(() => undefined)
      .finally(() => setConfirming(null))
  }

  return (
    <section className="flex w-full flex-col gap-5" data-testid="survey-admin-detail">
      <PageHeader
        title={
          <>
            {title} <StatusPill kind="survey" status={survey.status} />
          </>
        }
        backTo="/ops/surveys"
        backLabel={t('tour.back')}
        breadcrumbs={[{ label: t('surveyAdmin.title'), to: '/ops/surveys' }, { label: title }]}
        actions={
          next === 'live' ? (
            <Button
              type="button"
              data-testid="survey-publish"
              disabled={setStatus.isPending}
              onClick={() => setConfirming('live')}
            >
              {t('surveyAdmin.publish')}
            </Button>
          ) : next === 'closed' ? (
            <Button
              type="button"
              variant="secondary"
              data-testid="survey-close"
              disabled={setStatus.isPending}
              onClick={() => setConfirming('closed')}
            >
              {t('surveyAdmin.close')}
            </Button>
          ) : undefined
        }
      />

      <SurveyFacts survey={survey} />

      {setStatus.isError && (
        <div data-testid="survey-status-error">
          <ErrorState error={setStatus.error} onRetry={() => setStatus.reset()} />
        </div>
      )}

      {draft && admin ? (
        <>
          {/* Keyed by the stored version: a confirmed save comes back with a
              new updated_at, and the form starts again from what was saved. */}
          <SurveyEditor key={survey.updated_at} survey={survey} update={update} />
          <QuestionsEditor surveyId={survey.id} questions={questions} />
        </>
      ) : draft ? (
        <>
          <p className="type-body" style={{ color: 'var(--ink-2)' }}>
            {t('surveyAdmin.adminOnly')}
          </p>
          <QuestionList questions={questions} />
        </>
      ) : (
        <>
          <SurveyResults surveyId={survey.id} currency={survey.currency} questions={questions} summary={summary} />
          <SurveyVouchers surveyId={survey.id} />
        </>
      )}

      {confirming && (
        <ConfirmDialog
          title={t(confirming === 'live' ? 'surveyAdmin.publishTitle' : 'surveyAdmin.closeTitle')}
          detail={t(confirming === 'live' ? 'surveyAdmin.publishDetail' : 'surveyAdmin.closeDetail')}
          confirmLabel={t(confirming === 'live' ? 'surveyAdmin.publish' : 'surveyAdmin.close')}
          cancelLabel={t('surveyAdmin.goBack')}
          confirming={setStatus.isPending}
          onCancel={() => setConfirming(null)}
          onConfirm={() => change(confirming)}
        />
      )}
    </section>
  )
}

/** What the survey is, and who made it what it is. */
function SurveyFacts({ survey }: { survey: Survey }) {
  const { t } = useTranslation()
  const createdBy = useActorName(survey.created_by)
  const publishedBy = useActorName(survey.published_by)

  return (
    <div className="flex flex-col gap-2.5">
      <dl className="grid gap-x-5 gap-y-1.5 sm:grid-cols-2 xl:grid-cols-4">
        <Fact label={t('surveyAdmin.columns.reward')} value={formatMoney(survey.reward_amount, survey.currency)} testId="survey-fact-reward" />
        <Fact
          label={t('surveyAdmin.auditRate')}
          value={formatPercent(Number(survey.audit_rate) * 100)}
          testId="survey-fact-audit"
        />
        {survey.closes_at && (
          <Fact
            label={t('surveyAdmin.closesOn')}
            value={formatPlainDate(timestampToDarDate(survey.closes_at))}
            testId="survey-fact-closes"
          />
        )}
        {survey.max_households != null && (
          <Fact label={t('surveyAdmin.maxHouseholds')} value={String(survey.max_households)} testId="survey-fact-max-households" />
        )}
      </dl>
      <p className="type-note flex flex-wrap gap-x-4 gap-y-1" style={{ color: 'var(--ink-2)' }}>
        <span data-testid="survey-created-by">{t('surveyAdmin.createdBy', { name: createdBy ?? UNKNOWN })}</span>
        {survey.published_at && (
          <span data-testid="survey-published-by">
            {t('surveyAdmin.publishedBy', {
              date: formatTimestamp(survey.published_at),
              name: publishedBy ?? UNKNOWN,
            })}
          </span>
        )}
      </p>
    </div>
  )
}

function Fact({ label, value, testId }: { label: string; value: string; testId: string }) {
  return (
    <div className="grid items-baseline gap-3" style={{ gridTemplateColumns: 'minmax(0, 1fr) auto' }}>
      <dt style={{ fontSize: 13, color: 'var(--ink-2)' }}>{label}</dt>
      <dd data-testid={testId} className="tabular font-semibold" style={{ fontSize: 15 }}>
        {value}
      </dd>
    </div>
  )
}
