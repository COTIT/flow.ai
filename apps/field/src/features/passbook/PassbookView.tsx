import type { ReactNode } from 'react'
import type { EntryResource, Money, PayoutResource } from '@genesy/api-client/model'
import { useTranslation } from 'react-i18next'

import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Card, Loading } from '@/components/controls'
import type { Passbook } from '@/features/passbook/usePassbook'
import { formatPrice, formatTimestamp } from '@/lib/format'

interface Read<T> {
  isLoading: boolean
  error: unknown
  data: T | undefined
  refetch: () => unknown
}

/**
 * A passbook as the API states it: the balance per currency, the money lines newest first
 * (signed, with the balance after each) and the payouts. Display only; `action` adds a control
 * under each positive balance (the staff Pay out).
 */
export function PassbookView({
  passbook,
  payouts,
  action,
}: {
  passbook: Read<Passbook>
  payouts: Read<PayoutResource[]>
  action?: (balance: Money) => ReactNode
}) {
  const { t, i18n } = useTranslation()
  const money = (m: Money) => formatPrice(m, i18n.resolvedLanguage ?? 'en')

  if (passbook.error) return <ErrorState error={passbook.error} onRetry={() => void passbook.refetch()} />
  if (passbook.isLoading || !passbook.data) return <Loading testId="passbook-loading" />
  const { balances, lines } = passbook.data

  return (
    <div data-testid="passbook" className="flex flex-col gap-3.5">
      {lines.length === 0 ? (
        <div data-testid="passbook-empty">
          <EmptyState title={t('passbook.emptyTitle')} detail={t('passbook.emptyDetail')} />
        </div>
      ) : (
        <>
          <div className="flex flex-wrap gap-2.5">
            {balances.map((b) => (
              <div
                key={b.currency_id}
                className="flex min-w-0 flex-col gap-1.5 px-3 py-2.5"
                style={{ flex: '1 1 0', border: '1px solid var(--rule)', borderRadius: 'var(--radius-control)', background: 'var(--sand-2)' }}
              >
                <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                  {t('passbook.balance')}
                </p>
                <p data-testid={`passbook-balance-${b.currency}`} className="tabular" style={{ fontSize: 22, fontWeight: 600 }}>
                  {money(b)}
                </p>
                {action && b.minor > 0 ? action(b) : null}
              </div>
            ))}
          </div>
          <ul className="flex flex-col gap-2">
            {lines.map((l: EntryResource) => (
              <li key={l.id} data-testid="passbook-line">
                <Card className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                  <span className="font-semibold">{t(`passbook.kind.${l.kind}`, { defaultValue: t('common.other') })}</span>
                  <span className="tabular font-semibold">{money(l.amount)}</span>
                  {l.reason ? (
                    <span className="type-note w-full" data-testid="passbook-reason">
                      {t('passbook.reason', { reason: l.reason })}
                    </span>
                  ) : null}
                  <span className="type-note tabular w-full" style={{ color: 'var(--ink-2)' }}>
                    {formatTimestamp(l.created_at)} · {t('passbook.after', { balance: money(l.balance_after) })}
                  </span>
                </Card>
              </li>
            ))}
          </ul>
        </>
      )}

      <h3 className="type-section" style={{ color: 'var(--ink-3)' }}>
        {t('passbook.payouts')}
      </h3>
      {payouts.error ? (
        <ErrorState error={payouts.error} onRetry={() => void payouts.refetch()} />
      ) : payouts.isLoading || !payouts.data ? (
        <Loading testId="payouts-loading" />
      ) : payouts.data.length === 0 ? (
        <p data-testid="payouts-empty" className="type-note">
          {t('passbook.noPayouts')}
        </p>
      ) : (
        <ul className="flex flex-col gap-2">
          {payouts.data.map((p) => (
            <li key={p.id} data-testid="payout-row">
              <Card className="flex flex-col gap-1">
                <p className="flex flex-wrap justify-between gap-x-3">
                  <span className="tabular font-semibold">{money(p.amount)}</span>
                  <span className="type-note">{t(`passbook.idType.${p.id_type}`, { defaultValue: t('common.other') })}</span>
                </p>
                <p className="type-note tabular" style={{ color: 'var(--ink-2)' }}>
                  {formatTimestamp(p.paid_at)}
                </p>
                {p.void && (
                  <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                    {t('passbook.voided', { reason: p.void.reason })}
                  </p>
                )}
              </Card>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}
