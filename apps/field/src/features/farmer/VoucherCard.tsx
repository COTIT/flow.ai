import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { AuditTimeline } from '@/components/AuditTimeline'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { Loading, ProductNote } from '@/components/controls'
import { QrCode } from '@/components/QrCode'
import { StatusPill } from '@/components/StatusPill'
import {
  redeemedByName,
  useVoucher,
  useVoucherCode,
  useVoucherTimeline,
  voucherDisplayStatus,
  type FarmerVoucher,
} from '@/features/farmer/useVouchers'
import { formatMoney, formatTimestamp } from '@/lib/format'
import { formatVoucherCode, voucherQrPayload } from '@/lib/voucherCode'

/**
 * The household's incentive voucher: what to show at the Ruaha office, and
 * everything that has happened to it.
 *
 * An incentive is a fixed cash amount per household per survey, handed over
 * in cash at the office. The QR code is drawn only while the voucher can
 * still be collected — a used, cancelled or expired voucher must not look
 * usable. Whether a scan is accepted is decided at the office by the
 * database, never here.
 */
export function VoucherCard({ voucherId }: { voucherId: string }) {
  const { t } = useTranslation()
  const voucher = useVoucher(voucherId)
  const code = useVoucherCode(voucherId)
  const timeline = useVoucherTimeline(voucherId)
  // Read once per mount: expiry is a date, and a screen left open over the
  // boundary is corrected by the office's refusal, not by a ticking clock.
  const [now] = useState(() => Date.now())

  if (voucher.error) {
    return <ErrorState error={voucher.error} onRetry={() => void voucher.refetch()} />
  }
  if (voucher.isLoading) return <Loading testId="voucher-loading" />

  // Zero rows is an answer: RLS does not show this voucher.
  if (!voucher.data) {
    return <EmptyState title={t('empty.noAccessToThis')} detail={t('empty.noAccessDetail')} />
  }

  const row = voucher.data
  const status = voucherDisplayStatus(row, now)
  const collectable = status === 'issued'
  const events = timeline.data ?? []

  return (
    <section
      data-testid="voucher-card"
      data-status={status}
      className="flex flex-col gap-4 p-4"
      style={{
        border: '1px solid var(--rule)',
        borderRadius: 'var(--radius-card)',
        background: 'var(--paper)',
      }}
    >
      <header className="flex flex-wrap items-center justify-between gap-2.5">
        <h2 className="type-title">{t('voucher.title')}</h2>
        <StatusPill kind="voucher" status={status} />
      </header>

      {collectable && code.data && (
        // QrCode owns its white quiet zone in both themes; nothing to add.
        <div className="flex justify-center">
          <QrCode
            value={voucherQrPayload(code.data)}
            label={t('voucher.qrLabel', { code: formatVoucherCode(code.data) })}
          />
        </div>
      )}

      {(code.isLoading || code.data) && (
        <div className="flex flex-col items-center gap-1">
          <span className="type-section" style={{ color: 'var(--ink-3)' }}>
            {t('voucher.code')}
          </span>
          {code.isLoading ? (
            <span data-testid="voucher-code-loading" className="type-body" style={{ color: 'var(--ink-2)' }}>
              {t('common.loading')}
            </span>
          ) : (
            <span
              data-testid="voucher-code"
              className="type-figure tabular"
              style={{ letterSpacing: '0.08em', color: 'var(--ink)' }}
            >
              {formatVoucherCode(code.data!)}
            </span>
          )}
        </div>
      )}

      <dl
        className="flex flex-col gap-0.5 px-3.5 py-3"
        style={{
          border: '1px solid var(--primary)',
          borderRadius: 'var(--radius-control)',
          background: 'var(--primary-tint)',
        }}
      >
        <dt className="type-note" style={{ color: 'var(--primary-ink)' }}>
          {t('voucher.amount')}
        </dt>
        <dd data-testid="voucher-amount" className="type-figure tabular" style={{ color: 'var(--primary-ink)' }}>
          {formatMoney(row.amount, row.currency)}
        </dd>
        <dd className="type-small" style={{ color: 'var(--primary-ink)' }}>
          {t('surveys.incentiveNote')}
        </dd>
      </dl>

      <p
        data-testid="voucher-validity"
        className="type-body-strong"
        style={{ color: status === 'void' ? 'var(--flag-ink)' : 'var(--ink)', textWrap: 'pretty' }}
      >
        {validityLine(t, row, status, redeemedByName(events))}
      </p>

      {collectable && <ProductNote>{t('voucher.showAtOffice')}</ProductNote>}

      <section className="flex flex-col gap-1.5">
        <h3 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('auditTrail.title')}
        </h3>
        {timeline.error ? (
          <ErrorState error={timeline.error} onRetry={() => void timeline.refetch()} />
        ) : timeline.isLoading ? (
          <Loading testId="voucher-timeline-loading" />
        ) : (
          <AuditTimeline events={events} />
        )}
      </section>
    </section>
  )
}

type Translate = ReturnType<typeof useTranslation>['t']

/** The one sentence that says where this voucher stands. */
function validityLine(
  t: Translate,
  row: FarmerVoucher,
  status: ReturnType<typeof voucherDisplayStatus>,
  redeemer: string | null,
): string {
  switch (status) {
    case 'issued':
      return t('voucher.expires', { date: formatTimestamp(row.expires_at) })
    case 'expired':
      return t('voucher.expired', { date: formatTimestamp(row.expires_at) })
    case 'redeemed':
      // The name is the redeemed event's snapshot, never a raw user id.
      return t('voucher.redeemedBy', { date: formatTimestamp(row.redeemed_at), name: redeemer ?? '—' })
    case 'void':
      return t('voucher.voided', { reason: row.void_reason ?? '—' })
  }
}
