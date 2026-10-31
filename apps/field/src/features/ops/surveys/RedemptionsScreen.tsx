import { useMemo } from 'react'
import { getRouteApi, useNavigate } from '@tanstack/react-router'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { CONTROL } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { PageHeader } from '@/components/PageHeader'
import { TableSurface } from '@/components/TableSurface'
import { Field, SectionTitle } from '@/features/ops/surveys/Field'
import { UNKNOWN } from '@/features/ops/surveys/figures'
import { defaultRedemptionRange, validateRedemptionSearch } from '@/features/ops/surveys/surveyDates'
import {
  useRedemptions,
  type RedemptionLogRow,
  type RedemptionTotalRow,
} from '@/features/ops/surveys/useSurveyAdmin'
import { formatMoney, formatPlainDate, formatTimestamp } from '@/lib/format'
import { localisedField } from '@/lib/names'

const route = getRouteApi('/_ops/ops/surveys/redemptions')

type LogRow = RedemptionLogRow & { survey_title: string }

/**
 * Cash reconciliation: every incentive handed over in a range of Tanzanian
 * days, and the per-officer, per-day totals app_redemption_totals computes —
 * the figure an officer's cash is counted against. Nothing is summed here.
 *
 * The range lives in the URL, so a reconciliation view can be shared and
 * reloaded.
 */
export function RedemptionsScreen() {
  const { t, i18n } = useTranslation()
  const navigate = useNavigate()
  // Validated at the point of use as well as on the route: the RPC must never
  // receive an unvalidated date.
  const search = validateRedemptionSearch(route.useSearch())
  const fallback = defaultRedemptionRange()
  const from = search.from ?? fallback.from
  const to = search.to ?? fallback.to
  const query = useRedemptions(from, to)

  const setRange = (field: 'from' | 'to', value: string) => {
    void navigate({
      to: '/ops/surveys/redemptions',
      search: {
        from: field === 'from' ? value || undefined : from,
        to: field === 'to' ? value || undefined : to,
      },
      replace: true,
    })
  }

  const language = i18n.resolvedLanguage
  // Chosen into the rows: TanStack Table caches accessor values per row.
  const log = useMemo<LogRow[]>(
    () =>
      (query.data?.log ?? []).map((row) => ({
        ...row,
        survey_title: localisedField(row, 'survey_title', language),
      })),
    [query.data, language],
  )

  const totalColumns = useMemo(() => {
    const col = createColumnHelper<RedemptionTotalRow>()
    return [
      col.accessor('day', { header: t('surveyAdmin.totalsColumns.day'), cell: (c) => formatPlainDate(c.getValue()) }),
      col.accessor('redeemed_by_name', {
        header: t('surveyAdmin.totalsColumns.officer'),
        cell: (c) => c.getValue() ?? UNKNOWN,
      }),
      col.accessor('vouchers', { header: t('surveyAdmin.totalsColumns.vouchers'), meta: { numeric: true } }),
      col.accessor('amount', {
        header: t('surveyAdmin.totalsColumns.amount'),
        meta: { numeric: true },
        cell: (c) => <span className="font-semibold">{formatMoney(c.getValue(), c.row.original.currency)}</span>,
      }),
    ]
  }, [t])

  const logColumns = useMemo(() => {
    const col = createColumnHelper<LogRow>()
    return [
      col.accessor('redeemed_at', {
        header: t('surveyAdmin.logColumns.when'),
        cell: (c) => formatTimestamp(c.getValue()),
      }),
      col.accessor('redeemed_by_name', {
        header: t('surveyAdmin.logColumns.officer'),
        cell: (c) => c.getValue() ?? UNKNOWN,
      }),
      col.accessor('village_name', { header: t('surveyAdmin.logColumns.village') }),
      col.accessor('household_label', { header: t('surveyAdmin.logColumns.household') }),
      col.accessor('survey_title', { header: t('surveyAdmin.logColumns.survey') }),
      col.accessor('amount', {
        header: t('surveyAdmin.logColumns.amount'),
        meta: { numeric: true },
        cell: (c) => formatMoney(c.getValue(), c.row.original.currency),
      }),
      col.accessor('id_type_seen', {
        header: t('surveyAdmin.logColumns.idType'),
        cell: (c) => (c.getValue() ? t(`auditTrail.idType.${c.getValue()}`) : UNKNOWN),
      }),
    ]
  }, [t])

  const empty = { title: t('surveyAdmin.noRedemptions'), detail: t('surveyAdmin.noRedemptionsDetail') }

  return (
    <section className="flex w-full flex-col gap-5" data-testid="redemptions">
      <PageHeader
        title={t('surveyAdmin.redemptions')}
        description={t('surveyAdmin.redemptionsIntro')}
        backTo="/ops/surveys"
        backLabel={t('tour.back')}
        breadcrumbs={[{ label: t('surveyAdmin.title'), to: '/ops/surveys' }, { label: t('surveyAdmin.redemptions') }]}
      />

      <div className="flex flex-wrap items-end gap-3" data-testid="redemptions-range">
        <Field label={t('surveyAdmin.from')} htmlFor="redemptions-from">
          <input
            id="redemptions-from"
            data-testid="redemptions-from"
            type="date"
            value={from}
            onChange={(e) => setRange('from', e.target.value)}
            style={CONTROL}
          />
        </Field>
        <Field label={t('surveyAdmin.to')} htmlFor="redemptions-to">
          <input
            id="redemptions-to"
            data-testid="redemptions-to"
            type="date"
            value={to}
            onChange={(e) => setRange('to', e.target.value)}
            style={CONTROL}
          />
        </Field>
      </div>

      {query.error ? (
        <ErrorState error={query.error} onRetry={() => void query.refetch()} />
      ) : query.isLoading ? (
        <Loading testId="redemptions-loading" />
      ) : (
        <>
          <section className="flex flex-col gap-2.5">
            <SectionTitle>{t('surveyAdmin.totals')}</SectionTitle>
            <TableSurface>
              <DataTable
                columns={totalColumns}
                data={query.data?.totals ?? []}
                testId="redemption-totals-table"
                rowTestId="redemption-total-row"
                empty={empty}
              />
            </TableSurface>
          </section>

          <section className="flex flex-col gap-2.5">
            <SectionTitle>{t('surveyAdmin.redemptions')}</SectionTitle>
            <TableSurface>
              <DataTable
                columns={logColumns}
                data={log}
                testId="redemptions-table"
                rowTestId="redemption-row"
                empty={empty}
              />
            </TableSurface>
          </section>
        </>
      )}
    </section>
  )
}
