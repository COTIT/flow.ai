import { useMemo, useRef, useState } from 'react'
import { Link, useNavigate } from '@tanstack/react-router'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { formatMoney } from '@genesy/ui/lib/money'

import { useSession } from '@/app/session'
import { useWorkspaceId } from '@/app/workspace'
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
import { amountToMinor, countAndAmount } from '@/features/ops/surveys/figures'
import { canAuthorSurveys } from '@/features/ops/surveys/permissions'
import {
  useCreateSurvey,
  useCurrencies,
  useSurveyAdminList,
  type SurveyListRow,
} from '@/features/ops/surveys/useSurveyAdmin'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { localisedField } from '@/lib/names'
import { useFieldErrors } from '@/lib/useFieldErrors'
import { usePersistentForm } from '@/lib/usePersistentForm'

type ListRow = SurveyListRow & { title: string }

/**
 * Ops/admin — every survey, with the response count the API holds for it.
 *
 * Only an admin authors. The "New survey" control renders for an active admin
 * membership; the API's 403 is what actually refuses anyone else.
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
      col.accessor((row) => row.reward.minor, {
        id: 'reward',
        header: t('surveyAdmin.columns.reward'),
        meta: { numeric: true },
        cell: (c) => {
          const { minor, decimals, currency } = c.row.original.reward
          return formatMoney(minor, decimals, currency, i18n.language)
        },
      }),
      col.accessor('responses', {
        header: t('surveyAdmin.columns.responses'),
        meta: { numeric: true },
        cell: (c) => <span data-testid="survey-responses">{c.getValue()}</span>,
      }),
      col.accessor((row) => row.summary?.redeemed.minor ?? null, {
        id: 'redeemed',
        header: t('surveyAdmin.columns.redeemed'),
        meta: { numeric: true },
        cell: (c) => <span data-testid="survey-redeemed">{sums(c.row.original, 'redeemed', i18n.language)}</span>,
      }),
      col.accessor((row) => row.summary?.outstanding.minor ?? null, {
        id: 'outstanding',
        header: t('surveyAdmin.columns.outstanding'),
        meta: { numeric: true },
        cell: (c) => <span data-testid="survey-outstanding">{sums(c.row.original, 'outstanding', i18n.language)}</span>,
      }),
    ]
  }, [t, i18n.language])

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
          onClose={() => setCreateOpen(false)}
          onCreated={(surveyId: string) => void navigate({ to: '/ops/surveys/$surveyId', params: { surveyId } })}
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
              'data-answered': String(row.responses > 0),
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

/** The API's voucher sums for one survey (null for farmers, who never see this list). */
const sums = (row: ListRow, which: 'redeemed' | 'outstanding', language: string) =>
  countAndAmount(row.summary?.[which], row.reward.currency, row.reward.decimals, language)

const NEW_SURVEY_DEFAULTS: { title_en: string; reward_amount: string; currency_id: string } = {
  title_en: '',
  reward_amount: '',
  currency_id: '',
}

/**
 * Three fields, then the draft opens for everything else. Nothing is
 * pre-checked: the title and incentive checks are the API's, and its message
 * is what the admin reads. The typed amount becomes integer minor units of the
 * chosen currency by string maths (amountToMinor), never a float.
 */
function NewSurveyPanel({ onClose, onCreated }: { onClose: () => void; onCreated: (surveyId: string) => void }) {
  const { t } = useTranslation()
  const create = useCreateSurvey()
  const currencies = useCurrencies()
  const draft = usePersistentForm('survey-create', useWorkspaceId() ?? 'workspace', NEW_SURVEY_DEFAULTS)
  const [title, setTitle] = draft.field('title_en')
  const [reward, setReward] = draft.field('reward_amount')
  const [currencyId, setCurrencyId] = draft.field('currency_id')
  const list = currencies.data ?? []
  const currency =
    list.find((c) => c.id === currencyId) ?? list.find((c) => c.default_currency) ?? list[0]
  // isPending is only true on the NEXT render; a ref latches in the same tick.
  const inFlight = useRef(false)
  // The API's refusal of a field (a blank title, no reward) is shown under it.
  const refusal = useFieldErrors(create.error, draft.setError, (key) => NEW_SURVEY_FIELD[key], (field) => NEW_SURVEY_INPUT[field])
  const server = (field: keyof typeof NEW_SURVEY_DEFAULTS) => draft.formState.errors[field]?.message

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (inFlight.current || create.isPending || !draft.ready || !currency) return
    inFlight.current = true
    const save = create
      .mutateAsync({
        // Kept with the draft until it is saved: a retry after a lost reply gets the same survey back.
        id: draft.clientRef,
        title_en: title.trim(),
        // Null (blank or not an amount) goes as null: the API says what is wrong.
        reward_minor: amountToMinor(reward, currency.decimals),
        currency_id: currency.id,
      })
      .then((created) => onCreated(created.id))
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
          <Field label={t('surveyAdmin.fields.title_en')} htmlFor="survey-new-title" error={server('title_en')}>
            <input
              id="survey-new-title"
              data-testid="survey-new-title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.reward_amount')} htmlFor="survey-new-reward" error={server('reward_amount')}>
            <input
              id="survey-new-reward"
              data-testid="survey-new-reward"
              inputMode="decimal"
              value={reward}
              onChange={(e) => setReward(e.target.value)}
              className="w-full"
              style={CONTROL}
            />
          </Field>
          <Field label={t('surveyAdmin.fields.currency')} htmlFor="survey-new-currency" error={server('currency_id')}>
            <select
              id="survey-new-currency"
              data-testid="survey-new-currency"
              value={currency?.id ?? ''}
              onChange={(e) => setCurrencyId(e.target.value)}
              className="w-full"
              style={CONTROL}
            >
              {list.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.asset_type.toUpperCase()}
                </option>
              ))}
            </select>
          </Field>
        </div>

        {currencies.error && <ErrorState error={currencies.error} />}
        {create.isError && refusal != null && (
          <div data-testid="survey-create-error">
            <ErrorState error={refusal} />
          </div>
        )}

        <Button
          type="submit"
          data-testid="survey-new-submit"
          disabled={!draft.ready || !currency || create.isPending}
          className="w-fit"
        >
          {create.isPending ? t('surveyAdmin.saving') : t('surveyAdmin.save')}
        </Button>
      </form>
    </Panel>
  )
}

/** The form field each API key belongs to, and its input. */
const NEW_SURVEY_FIELD: Record<string, string> = { title_en: 'title_en', reward_minor: 'reward_amount', currency_id: 'currency_id' }
const NEW_SURVEY_INPUT: Record<string, string> = { title_en: 'survey-new-title', reward_amount: 'survey-new-reward', currency_id: 'survey-new-currency' }
