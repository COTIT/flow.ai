import { useMemo, useRef, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { useSession } from '@/app/session'
import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { CONTROL } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { TableSurface } from '@/components/TableSurface'
import { Button } from '@/components/ui/button'
import { Field, Panel, SectionTitle } from '@/features/ops/surveys/Field'
import { countAndAmount, UNKNOWN } from '@/features/ops/surveys/figures'
import { authoringProjectId, canAuthorSurveys } from '@/features/ops/surveys/permissions'
import {
  useCreateSurvey,
  useSurveyAdminList,
  type SurveyListRow,
} from '@/features/ops/surveys/useSurveyAdmin'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { formatMoney } from '@/lib/format'
import { localisedField } from '@/lib/names'
import { usePersistentForm } from '@/lib/usePersistentForm'

type ListRow = SurveyListRow & { title: string }

/**
 * Ops/admin — every survey, with the figures v_survey_summary holds for it.
 *
 * Only an admin authors. The "New survey" control renders for an active admin
 * membership; survey_guard and the insert policy are what actually refuse
 * anyone else.
 */
export function SurveyAdminListScreen() {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  const query = useSurveyAdminList()
  const { data: session } = useSession()
  const admin = canAuthorSurveys(session?.memberships)
  const [createOpen, setCreateOpen] = useState(false)

  // The title is chosen here, at render, into the rows themselves: TanStack
  // Table caches an accessor's value per row, so a language-dependent
  // accessor over unchanged data would keep serving the first language.
  const language = i18n.resolvedLanguage
  const rows = useMemo<ListRow[]>(
    () => (query.data ?? []).map((row) => ({ ...row, title: localisedField(row, 'title', language) })),
    [query.data, language],
  )

  const columns = useMemo(() => {
    const col = createColumnHelper<ListRow>()
    return [
      col.accessor('title', {
        header: t('surveyAdmin.columns.title'),
        cell: (c) => <span className="font-semibold">{c.getValue()}</span>,
      }),
      col.accessor('status', {
        header: t('surveyAdmin.columns.status'),
        cell: (c) => <StatusPill kind="survey" status={c.getValue()} />,
      }),
      col.accessor('reward_amount', {
        header: t('surveyAdmin.columns.reward'),
        meta: { numeric: true },
        cell: (c) => formatMoney(c.getValue(), c.row.original.currency),
      }),
      col.accessor((row) => row.summary?.responses ?? null, {
        id: 'responses',
        header: t('surveyAdmin.columns.responses'),
        meta: { numeric: true },
        cell: (c) => <span data-testid="survey-responses">{c.getValue() ?? UNKNOWN}</span>,
      }),
      col.accessor((row) => row.summary?.redeemed_amount ?? null, {
        id: 'redeemed',
        header: t('surveyAdmin.columns.redeemed'),
        meta: { numeric: true },
        cell: (c) => (
          <span data-testid="survey-redeemed">
            {countAndAmount(c.row.original.summary?.redeemed_count, c.getValue(), c.row.original.currency)}
          </span>
        ),
      }),
      col.accessor((row) => row.summary?.outstanding_amount ?? null, {
        id: 'outstanding',
        header: t('surveyAdmin.columns.outstanding'),
        meta: { numeric: true },
        cell: (c) => (
          <span data-testid="survey-outstanding">
            {countAndAmount(c.row.original.summary?.outstanding_count, c.getValue(), c.row.original.currency)}
          </span>
        ),
      }),
    ]
  }, [t])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  return (
    <section className="flex flex-col gap-5" data-testid="survey-admin-list">
      <PageHeader
        title={t('surveyAdmin.title')}
        description={t('surveyAdmin.intro')}
        actions={
          <>
            <Link
              to="/ops/surveys/redemptions"
              data-testid="surveys-redemptions-link"
              className="inline-flex min-h-11 items-center justify-center rounded-[var(--radius-control)] border border-rule-2 bg-paper px-4 text-sm font-medium text-ink hover:bg-sand-2 focus-visible:outline-2 focus-visible:outline-primary focus-visible:outline-offset-2"
            >
              {t('surveyAdmin.redemptions')}
            </Link>
            {admin && (
              <Button
                type="button"
                data-testid="survey-create-open"
                aria-expanded={createOpen}
                aria-controls="survey-create-panel"
                variant={createOpen ? 'secondary' : 'primary'}
                onClick={() => setCreateOpen((open) => !open)}
              >
                {createOpen ? t('common.close') : t('surveyAdmin.new')}
              </Button>
            )}
          </>
        }
      />

      {admin && createOpen && (
        <NewSurveyPanel
          projectId={authoringProjectId(session?.memberships)}
          onClose={() => setCreateOpen(false)}
          onCreated={(surveyId) => void navigate({ to: '/ops/surveys/$surveyId', params: { surveyId } })}
        />
      )}

      {query.isLoading ? (
        <Loading testId="surveys-loading" />
      ) : (
        <TableSurface>
          <DataTable
            columns={columns}
            data={rows}
            testId="surveys-table"
            rowTestId="survey-row"
            // What the guided tour needs to open the right row: a live survey
            // that somebody has answered, not whichever one happens to be first.
            rowAttrs={(row) => ({
              'data-status': row.status,
              'data-answered': String((row.summary?.responses ?? 0) > 0),
            })}
            onRowClick={(row) =>
              void navigate({ to: '/ops/surveys/$surveyId', params: { surveyId: row.id } })
            }
            empty={{ title: t('surveyAdmin.empty') }}
          />
        </TableSurface>
      )}
    </section>
  )
}

const NEW_SURVEY_DEFAULTS: { title_en: string; reward_amount: string } = { title_en: '', reward_amount: '' }

/**
 * Two fields, then the draft opens for everything else. Nothing is
 * pre-checked: the title and incentive checks are the database's, and its
 * message is what the admin reads.
 */
function NewSurveyPanel({
  projectId,
  onClose,
  onCreated,
}: {
  projectId: string | undefined
  onClose: () => void
  onCreated: (surveyId: string) => void
}) {
  const { t } = useTranslation()
  const create = useCreateSurvey()
  const draft = usePersistentForm('survey-create', projectId ?? 'project', NEW_SURVEY_DEFAULTS)
  const [title, setTitle] = draft.field('title_en')
  const [reward, setReward] = draft.field('reward_amount')
  // isPending is only true on the NEXT render; a ref latches in the same tick.
  const inFlight = useRef(false)

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (inFlight.current || create.isPending || !draft.ready || !projectId) return
    inFlight.current = true
    const id = draft.clientRef
    const save = create
      .mutateAsync({
        id,
        project_id: projectId,
        title_en: title.trim(),
        reward_amount: reward.trim() === '' ? null : Number(reward),
      })
      .then(() => onCreated(id))
    void finishDraftWhenSaved(save, draft.finish).finally(() => {
      inFlight.current = false
    })
  }

  return (
    <Panel id="survey-create-panel" testId="survey-create-panel">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SectionTitle>{t('surveyAdmin.new')}</SectionTitle>
        <Button type="button" variant="ghost" size="sm" data-testid="survey-create-close" onClick={onClose}>
          {t('common.close')}
        </Button>
      </div>
      <form className="flex flex-col gap-3" noValidate onSubmit={submit}>
        <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <Field label={t('surveyAdmin.fields.title_en')} htmlFor="survey-new-title">
            <input
              id="survey-new-title"
              data-testid="survey-new-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.reward_amount')} htmlFor="survey-new-reward">
            <input
              id="survey-new-reward"
              data-testid="survey-new-reward"
              type="number"
              inputMode="decimal"
              min={0}
              value={reward}
              onChange={(e) => setReward(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
        </div>

        {create.isError && <ErrorState error={create.error} />}

        <Button
          type="submit"
          data-testid="survey-new-submit"
          disabled={!draft.ready || create.isPending}
          className="w-fit"
        >
          {create.isPending ? t('surveyAdmin.saving') : t('surveyAdmin.save')}
        </Button>
      </form>
    </Panel>
  )
}
