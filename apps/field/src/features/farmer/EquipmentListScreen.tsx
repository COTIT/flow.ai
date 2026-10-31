import { Link, getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { OfferedPills } from '@/components/OfferedPills'
import { ResourceTabs } from '@/components/ResourceTabs'
import { IndicativePill, Loading } from '@/components/controls'
import { kindOf } from '@/features/farmer/resourceKind'
import { useEquipmentList, type EquipmentItem } from '@/features/farmer/useEquipment'
import { useLoanProducts } from '@/features/farmer/useResources'
import { formatKw, formatMoney } from '@/lib/format'

const route = getRouteApi('/_farmer/farm/equipment/')

const CARD = {
  border: '1px solid var(--rule)',
  borderRadius: 'var(--radius-card)',
  background: 'var(--paper)',
  color: 'var(--ink)',
}

/**
 * Spec 6.3 — the resource catalogue, as the farmer sees it: equipment to
 * rent or buy, and loan listings. Every price and amount is labelled
 * indicative. A loan is a listing only: it is not an offer and cannot be
 * requested here — the farmer asks at the office.
 */
export function EquipmentListScreen() {
  const { t } = useTranslation()
  const search = route.useSearch()
  const navigate = route.useNavigate()
  const kind = kindOf(search)

  return (
    <section className="flex flex-col gap-3.5" data-testid="equipment-list">
      <header className="flex flex-col gap-1.5">
        <h1 className="type-screen-title">{t('resources.title')}</h1>
        <p style={{ fontSize: 13, lineHeight: 1.5, color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t(kind === 'loan' ? 'resources.loanNote' : 'equipment.notAQuotation')}
        </p>
      </header>

      <ResourceTabs
        value={kind}
        onChange={(next) => void navigate({ search: next === 'loan' ? { kind: 'loan' } : {} })}
        testIdPrefix="resources"
      />

      {kind === 'equipment' ? <EquipmentCards /> : <LoanCards />}
    </section>
  )
}

function EquipmentCards() {
  const { t } = useTranslation()
  const query = useEquipmentList()

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />
  if (query.isLoading) return <Loading testId="equipment-loading" />
  if (query.items.length === 0) {
    return <EmptyState title={t('equipment.noneTitle')} detail={t('equipment.noneDetail')} />
  }

  return (
    <ul className="flex flex-col gap-2.5">
      {query.items.map((item) => (
        <li key={item.id} data-testid="equipment-item">
          <Link
            to="/farm/equipment/$equipmentId"
            params={{ equipmentId: item.id }}
            data-testid={`equipment-card-${item.id}`}
            className="flex flex-wrap items-center justify-between gap-3 p-4 hover:bg-primary-tint"
            style={{ ...CARD, minHeight: 52 }}
          >
            <span className="flex min-w-0 flex-col gap-1">
              <b data-testid="equipment-card" style={{ fontSize: 16, fontWeight: 600 }}>
                {item.name}
              </b>
              <span className="type-note" style={{ color: 'var(--ink-3)' }}>
                {item.category_name} · {formatKw(item.rated_power_kw)}
              </span>
              <OfferedPills canRent={item.can_rent} canBuy={item.can_buy} testId="equipment-offered" />
            </span>
            <EquipmentPrices item={item} />
          </Link>
        </li>
      ))}
    </ul>
  )
}

/** "The word indicative appears next to every price" — and every rent. */
function EquipmentPrices({ item }: { item: EquipmentItem }) {
  const { t } = useTranslation()
  return (
    <span className="flex flex-col items-end gap-1">
      {item.can_buy && item.indicative_price !== null && (
        <span
          data-testid="equipment-price"
          className="tabular inline-flex flex-wrap items-center justify-end gap-2"
          style={{ fontSize: 15, fontWeight: 600 }}
        >
          {formatMoney(item.indicative_price, item.currency)}
          <IndicativePill />
        </span>
      )}
      {item.can_rent && item.indicative_rent_per_day !== null && (
        <span
          data-testid="equipment-rent"
          className="tabular inline-flex flex-wrap items-center justify-end gap-2"
          style={{ fontSize: 14, fontWeight: 600, color: 'var(--ink-2)' }}
        >
          {t('resources.rentPerDay')}: {formatMoney(item.indicative_rent_per_day, item.currency)}
          <IndicativePill />
        </span>
      )}
    </span>
  )
}

function LoanCards() {
  const { t } = useTranslation()
  const query = useLoanProducts()

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />
  if (query.isLoading) return <Loading testId="loans-loading" />
  if (query.items.length === 0) {
    return <EmptyState title={t('resources.noLoansTitle')} detail={t('resources.noLoansDetail')} />
  }

  return (
    <div className="flex flex-col gap-2.5">
      <p data-testid="loan-note" className="type-note" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>
        {t('resources.loanNote')}
      </p>
      <ul className="flex flex-col gap-2.5">
        {query.items.map((loan) => (
          <li key={loan.id} data-testid="loan-item" className="flex flex-col gap-1.5 p-4" style={CARD}>
            <b style={{ fontSize: 16, fontWeight: 600 }}>{loan.name}</b>
            {loan.description && (
              <p style={{ fontSize: 14, lineHeight: 1.5, color: 'var(--ink-2)', textWrap: 'pretty' }}>{loan.description}</p>
            )}
            <span
              data-testid="loan-range"
              className="tabular inline-flex flex-wrap items-center gap-2"
              style={{ fontSize: 15, fontWeight: 600 }}
            >
              {formatMoney(loan.indicative_min_amount, loan.currency)} – {formatMoney(loan.indicative_max_amount, loan.currency)}
              <IndicativePill />
            </span>
            <span className="type-note font-medium" style={{ color: 'var(--primary-ink)' }}>
              {t('resources.askAtOffice')}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}
