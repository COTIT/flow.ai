import { useMemo, useState } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { createColumnHelper } from '@tanstack/react-table'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { OfferedPills } from '@/components/OfferedPills'
import { PageHeader } from '@/components/PageHeader'
import { ResourceTabs } from '@/components/ResourceTabs'
import { TableSurface } from '@/components/TableSurface'
import { IndicativePill, Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
// Shared reference data, not a farmer-only concern: the same catalogue rows
// drive the farmer's Resources screen (§6.3) and this ops list (§7.4), scoped
// the same way by app_projects(). One hook per kind, two readers.
import { useEquipmentList, type EquipmentItem } from '@/features/farmer/useEquipment'
import { useLoanProducts, type LoanItem } from '@/features/farmer/useResources'
import { kindOf, type ResourceKind } from '@/features/farmer/resourceKind'
import { CatalogueCreatePanel } from '@/features/ops/CatalogueCreatePanel'
import { formatKw, formatMoney } from '@/lib/format'

/** A figure that is genuinely absent reads as absent, never as zero. */
const DASH = '—'

const route = getRouteApi('/_ops/ops/catalogue')

/**
 * Spec 7.4 — `/ops/catalogue`, the RESOURCE catalogue (5 Oct 2026): equipment
 * offered to rent and/or buy, and loan listings. Ops adds rows; the database
 * decides what a row may hold.
 *
 * `rated_power_kw` and every price are nullable. A null rated power causes
 * `pue_recompute_estimate` to DELETE the estimate rather than zero it
 * (business-rules §3), so null renders as absent, never as "0.000 kW".
 *
 * Loans are LISTINGS: a name, a description and an indicative range. No
 * finance terms — research item C is open — and never an offer.
 */
export function CatalogueScreen() {
  const { t } = useTranslation()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const kind = kindOf(search)
  const [createOpen, setCreateOpen] = useState(false)

  const setKind = (next: ResourceKind) => {
    setCreateOpen(false)
    void navigate({ search: next === 'loan' ? { kind: 'loan' } : {} })
  }

  return (
    <section className="flex flex-col gap-4">
      <PageHeader
        title={t('catalogue.title')}
        description={<span data-testid="catalogue-note">{t('catalogue.note')}</span>}
        actions={
          <Button
            type="button"
            data-testid="catalogue-create-open"
            aria-expanded={createOpen}
            aria-controls="catalogue-create-panel"
            variant={createOpen ? 'secondary' : 'primary'}
            onClick={() => setCreateOpen((open) => !open)}
          >
            {createOpen ? t('common.close') : t(`catalogue.add.${kind}`)}
            {createOpen ? <ChevronUp aria-hidden size={16} /> : <ChevronDown aria-hidden size={16} />}
          </Button>
        }
      />

      {/* A literal anchor for the guided tour, which cannot point at a templated id. */}
      <div data-testid="catalogue-kinds" className="w-fit">
        <ResourceTabs value={kind} onChange={setKind} testIdPrefix="catalogue" />
      </div>

      {createOpen && <CatalogueCreatePanel kind={kind} onClose={() => setCreateOpen(false)} />}

      {kind === 'equipment' ? <EquipmentTable /> : <LoanTable />}
    </section>
  )
}

function EquipmentTable() {
  const { t } = useTranslation()
  const query = useEquipmentList()

  const columns = useMemo(() => {
    const col = createColumnHelper<EquipmentItem>()
    const money = (value: number | null, currency: string, testId: string) => (
      /* The tag travels with the number, and absent stays absent. */
      <span data-testid={testId} className="inline-flex flex-wrap items-baseline justify-end gap-2">
        {value === null ? (
          <span className="tabular">{DASH}</span>
        ) : (
          <>
            <span className="tabular font-semibold">{formatMoney(value, currency)}</span>
            <IndicativePill />
          </>
        )}
      </span>
    )
    return [
      col.accessor('name', { header: t('catalogue.colName') }),
      col.accessor('category_name', { header: t('catalogue.colCategory') }),
      col.display({
        id: 'offered',
        header: t('catalogue.colOffered'),
        cell: (c) => <OfferedPills canRent={c.row.original.can_rent} canBuy={c.row.original.can_buy} testId="catalogue-offered" />,
      }),
      col.accessor('rated_power_kw', {
        meta: { numeric: true },
        header: t('catalogue.colPower'),
        cell: (c) => <span className="tabular">{c.getValue() === null ? DASH : formatKw(c.getValue() as number)}</span>,
      }),
      col.accessor('typical_hours_per_day', {
        meta: { numeric: true },
        header: t('catalogue.colHours'),
        cell: (c) => <span className="tabular">{c.getValue() ?? DASH}</span>,
      }),
      col.accessor('typical_days_per_week', {
        meta: { numeric: true },
        header: t('catalogue.colDays'),
        cell: (c) => <span className="tabular">{c.getValue() ?? DASH}</span>,
      }),
      col.accessor('indicative_price', {
        header: t('catalogue.colPrice'),
        meta: { numeric: true },
        cell: (c) => money(c.getValue(), c.row.original.currency, 'catalogue-price'),
      }),
      col.accessor('indicative_rent_per_day', {
        header: t('catalogue.colRent'),
        meta: { numeric: true },
        cell: (c) => money(c.getValue(), c.row.original.currency, 'catalogue-rent'),
      }),
    ]
  }, [t])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />
  if (query.isLoading) return <Loading testId="catalogue-loading" />

  return (
    <TableSurface>
      <DataTable
        columns={columns}
        data={query.items}
        testId="catalogue-table"
        rowTestId="catalogue-row"
        empty={{ title: t('equipment.noneTitle'), detail: t('equipment.noneDetail') }}
      />
    </TableSurface>
  )
}

function LoanTable() {
  const { t } = useTranslation()
  const query = useLoanProducts()

  const columns = useMemo(() => {
    const col = createColumnHelper<LoanItem>()
    return [
      col.accessor('name', { header: t('catalogue.colLoan') }),
      col.accessor('description', {
        header: t('catalogue.colDescription'),
        cell: (c) => c.getValue() ?? DASH,
      }),
      col.display({
        id: 'range',
        header: t('catalogue.colRange'),
        meta: { numeric: true },
        cell: (c) => (
          <span data-testid="catalogue-loan-range" className="inline-flex flex-wrap items-baseline justify-end gap-2">
            <span className="tabular font-semibold">
              {formatMoney(c.row.original.indicative_min_amount, c.row.original.currency)} –{' '}
              {formatMoney(c.row.original.indicative_max_amount, c.row.original.currency)}
            </span>
            <IndicativePill />
          </span>
        ),
      }),
    ]
  }, [t])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />
  if (query.isLoading) return <Loading testId="catalogue-loading" />

  return (
    <div className="flex flex-col gap-3">
      <p data-testid="catalogue-loan-note" className="type-note" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
        {t('resources.loanNote')}
      </p>
      <TableSurface>
        <DataTable
          columns={columns}
          data={query.items}
          testId="catalogue-loans-table"
          rowTestId="catalogue-loan-row"
          empty={{ title: t('resources.noLoansTitle'), detail: t('resources.noLoansDetail') }}
        />
      </TableSurface>
    </div>
  )
}
