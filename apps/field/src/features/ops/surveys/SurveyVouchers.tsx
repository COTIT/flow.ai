import { useMemo, useState } from 'react'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { AuditTimeline } from '@/components/AuditTimeline'
import { ConfirmDialog } from '@/components/ConfirmDialog'
import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { CONTROL } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { StatusPill } from '@/components/StatusPill'
import { TableSurface } from '@/components/TableSurface'
import { Button } from '@/components/ui/button'
import { Field, Panel, SectionTitle } from '@/features/ops/surveys/Field'
import { UNKNOWN } from '@/features/ops/surveys/figures'
import {
  useSurveyVouchers,
  useVoidVoucher,
  useVoucherTimeline,
  type SurveyVoucher,
} from '@/features/ops/surveys/useSurveyAdmin'
import { formatMoney, formatTimestamp } from '@/lib/format'

/**
 * Every voucher issued for one survey, from app_survey_vouchers (ops/admin).
 *
 * Choosing one shows its whole trail — app_voucher_timeline gives ops the
 * full record, audit hold and refused scans included — and, while it is still
 * issued, the one action ops have on it: cancelling it, with a reason.
 */
export function SurveyVouchers({ surveyId }: { surveyId: string }) {
  const { t } = useTranslation()
  const vouchers = useSurveyVouchers(surveyId)
  const [selectedId, setSelectedId] = useState<string | null>(null)
  // Read from the fresh list, so a cancelled voucher's panel follows the refetch.
  const selected = (vouchers.data ?? []).find((v) => v.voucher_id === selectedId) ?? null
  const timeline = useVoucherTimeline(selected?.voucher_id ?? null)

  const columns = useMemo(() => {
    const col = createColumnHelper<SurveyVoucher>()
    return [
      col.accessor('household_label', { header: t('surveyAdmin.voucherColumns.household') }),
      col.accessor('respondent_name', { header: t('surveyAdmin.voucherColumns.respondent') }),
      col.accessor('submitted_at', {
        header: t('surveyAdmin.voucherColumns.submitted'),
        cell: (c) => formatTimestamp(c.getValue()),
      }),
      col.accessor('status', {
        header: t('surveyAdmin.voucherColumns.status'),
        cell: (c) => <StatusPill kind="voucher" status={c.row.original.expired ? 'expired' : c.getValue()} />,
      }),
      col.accessor('amount', {
        header: t('surveyAdmin.voucherColumns.amount'),
        meta: { numeric: true },
        cell: (c) => formatMoney(c.getValue(), c.row.original.currency),
      }),
      col.accessor('redeemed_by_name', {
        header: t('surveyAdmin.voucherColumns.redeemedBy'),
        cell: (c) => c.getValue() ?? UNKNOWN,
      }),
      col.accessor('audit_required', {
        header: t('surveyAdmin.voucherColumns.audit'),
        cell: (c) => (c.getValue() ? <HeldMark /> : null),
      }),
    ]
  }, [t])

  return (
    <section className="flex flex-col gap-3" data-testid="survey-vouchers">
      <SectionTitle>{t('surveyAdmin.vouchers')}</SectionTitle>

      {vouchers.error ? (
        <ErrorState error={vouchers.error} onRetry={() => void vouchers.refetch()} />
      ) : vouchers.isLoading ? (
        <Loading testId="vouchers-loading" />
      ) : (
        <TableSurface>
          <DataTable
            columns={columns}
            data={vouchers.data ?? []}
            testId="vouchers-table"
            rowTestId="voucher-row"
            onRowClick={(row) => setSelectedId(row.voucher_id)}
            empty={{ title: t('surveyAdmin.noVouchers'), detail: t('surveyAdmin.noVouchersDetail') }}
          />
        </TableSurface>
      )}

      {selected ? (
        <VoucherPanel surveyId={surveyId} voucher={selected} timeline={timeline} />
      ) : (
        (vouchers.data?.length ?? 0) > 0 && (
          <p data-testid="voucher-select-prompt" className="type-note" style={{ color: 'var(--ink-3)' }}>
            {t('surveyAdmin.selectVoucher')}
          </p>
        )
      )}
    </section>
  )
}

function HeldMark() {
  const { t } = useTranslation()
  return (
    <span
      data-testid="voucher-held"
      className="type-note inline-flex items-center px-2 py-0.5 font-semibold"
      style={{
        border: '1px solid var(--rule-2)',
        borderRadius: 'var(--radius-pill)',
        background: 'var(--primary-tint)',
        color: 'var(--primary-ink)',
      }}
    >
      {t('surveyAdmin.held')}
    </span>
  )
}

function VoucherPanel({
  surveyId,
  voucher,
  timeline,
}: {
  surveyId: string
  voucher: SurveyVoucher
  timeline: ReturnType<typeof useVoucherTimeline>
}) {
  const { t } = useTranslation()

  return (
    <Panel testId="voucher-panel">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h3 className="type-body-strong">{t('surveyAdmin.voucherFor', { household: voucher.household_label })}</h3>
        <StatusPill kind="voucher" status={voucher.expired ? 'expired' : voucher.status} />
      </div>

      <div className="flex flex-col gap-2">
        <SectionTitle>{t('auditTrail.title')}</SectionTitle>
        {timeline.error ? (
          <ErrorState error={timeline.error} onRetry={() => void timeline.refetch()} />
        ) : timeline.isLoading ? (
          <Loading />
        ) : (
          <AuditTimeline events={timeline.data ?? []} />
        )}
      </div>

      {/* Only an issued voucher can be voided; the RPC holds that rule too. An
          expired one is still `issued` — it was never collected. */}
      {voucher.status === 'issued' && <VoidVoucher key={voucher.voucher_id} surveyId={surveyId} voucherId={voucher.voucher_id} />}
    </Panel>
  )
}

function VoidVoucher({ surveyId, voucherId }: { surveyId: string; voucherId: string }) {
  const { t } = useTranslation()
  const cancel = useVoidVoucher(surveyId)
  const [reason, setReason] = useState('')
  const [confirming, setConfirming] = useState(false)
  const reasonId = `void-reason-${voucherId}`

  const confirm = () => {
    cancel
      .mutateAsync({ voucherId, reason })
      .then(() => setReason(''))
      // The mutation's error state shows the refusal.
      .catch(() => undefined)
      .finally(() => setConfirming(false))
  }

  return (
    <div className="flex flex-col gap-2.5 pt-3" style={{ borderTop: '1px solid var(--rule)' }}>
      <Field label={t('surveyAdmin.voidReason')} htmlFor={reasonId}>
        <textarea
          id={reasonId}
          data-testid="voucher-void-reason"
          rows={2}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          className="w-full"
          style={CONTROL}
        />
      </Field>

      {cancel.isError && <ErrorState error={cancel.error} />}

      <Button
        type="button"
        variant="danger"
        data-testid="voucher-void"
        className="w-fit"
        disabled={cancel.isPending}
        onClick={() => setConfirming(true)}
      >
        {t('surveyAdmin.void')}
      </Button>

      {confirming && (
        <ConfirmDialog
          title={t('surveyAdmin.voidTitle')}
          detail={t('surveyAdmin.voidDetail')}
          confirmLabel={t('surveyAdmin.voidConfirm')}
          cancelLabel={t('surveyAdmin.goBack')}
          confirming={cancel.isPending}
          onCancel={() => setConfirming(false)}
          onConfirm={confirm}
        />
      )}
    </div>
  )
}
