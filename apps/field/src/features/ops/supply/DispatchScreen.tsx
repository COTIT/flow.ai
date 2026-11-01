import { useRef, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { FormField } from '@/components/FormField'
import { PageHeader } from '@/components/PageHeader'
import { CONTROL } from '@/components/controlStyles'
import { Card, Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
import {
  type DispatchInput,
  useDispatch,
  useDispatchOptions,
  useSupplyLinks,
  useSupplyTransfers,
} from '@/features/ops/supply/useSupply'
import { formatTimestamp } from '@/lib/format'

/** `/ops/dispatch`: send lot stock to a shop over a live link, and the transfers sent so far. */
export function DispatchScreen() {
  const { t } = useTranslation()
  const links = useSupplyLinks()
  const options = useDispatchOptions()
  const transfers = useSupplyTransfers()
  const dispatch = useDispatch()

  const [input, setInput] = useState<DispatchInput>({ linkId: '', lotId: '', warehouseId: '', quantity: '' })
  const [confirming, setConfirming] = useState(false)
  /** Set on the tap itself: a second tap before React re-renders the button disabled must not send twice. */
  const sending = useRef(false)

  const live = (links.data ?? []).filter((l) => l.status === 'live')
  const set = (key: keyof DispatchInput) => (e: { target: { value: string } }) =>
    setInput((now) => ({ ...now, [key]: e.target.value }))
  const ready = Object.values(input).every((v) => v.trim() !== '')

  const confirm = () => {
    if (sending.current) return
    sending.current = true
    dispatch.mutate(input, {
      onSuccess: () => setInput((now) => ({ ...now, quantity: '' })),
      onSettled: () => {
        sending.current = false
        setConfirming(false)
      },
    })
  }

  const loadError = links.error ?? options.error
  const select = (key: keyof DispatchInput, id: string, label: string, rows: { id: string; label: string }[]) => (
    <FormField label={label} id={id}>
      <select
        id={id}
        data-testid={id}
        value={input[key]}
        onChange={set(key)}
        className="w-full"
        style={CONTROL}
      >
        <option value="">{t('supply.choose')}</option>
        {rows.map((r) => (
          <option key={r.id} value={r.id}>
            {r.label}
          </option>
        ))}
      </select>
    </FormField>
  )

  return (
    <section data-testid="dispatch-screen" className="flex flex-col gap-4">
      <PageHeader title={t('supply.dispatchTitle')} description={t('supply.dispatchIntro')} />

      {loadError ? (
        <ErrorState error={loadError} />
      ) : links.isLoading || options.isLoading ? (
        <Loading testId="dispatch-loading" />
      ) : (
        <Card>
          <form
            data-testid="dispatch-form"
            className="grid gap-3 sm:grid-cols-2"
            onSubmit={(e: FormEvent) => {
              e.preventDefault()
              if (ready) setConfirming(true)
            }}
          >
            {select('linkId', 'dispatch-link', t('supply.link'), live.map((l) => ({ id: l.id, label: l.partner.name })))}
            {select('lotId', 'dispatch-lot', t('supply.lot'), (options.data?.lots ?? []).map((l) => ({ id: l.id, label: l.code })))}
            {select(
              'warehouseId',
              'dispatch-warehouse',
              t('supply.warehouse'),
              (options.data?.warehouses ?? []).map((w) => ({ id: w.id, label: w.name })),
            )}
            <FormField label={t('supply.quantity')} id="dispatch-quantity">
              <input
                id="dispatch-quantity"
                data-testid="dispatch-quantity"
                inputMode="decimal"
                value={input.quantity}
                onChange={set('quantity')}
                className="w-full"
                style={CONTROL}
                autoComplete="off"
              />
            </FormField>
            {dispatch.error ? (
              <div data-testid="dispatch-error" className="sm:col-span-2">
                <ErrorState error={dispatch.error} />
              </div>
            ) : dispatch.isSuccess ? (
              <p data-testid="dispatch-sent" role="status" className="sm:col-span-2">
                {t('supply.sent')}
              </p>
            ) : null}
            <div className="sm:col-span-2">
              <Button type="submit" data-testid="dispatch-send" disabled={!ready || dispatch.isPending}>
                {dispatch.isPending ? t('supply.sending') : t('supply.send')}
              </Button>
            </div>
          </form>
        </Card>
      )}

      <h2 className="font-semibold">{t('supply.transfersTitle')}</h2>
      {transfers.error ? (
        <ErrorState error={transfers.error} onRetry={() => void transfers.refetch()} />
      ) : transfers.isLoading ? (
        <Loading testId="supply-transfers-loading" />
      ) : !transfers.data?.length ? (
        <EmptyState title={t('supply.noTransfersTitle')} detail={t('supply.noTransfersDetail')} />
      ) : (
        <ul className="flex flex-col gap-2">
          {transfers.data.map((tr) => (
            <li key={tr.id} data-testid="supply-transfer-row">
              <Card className="flex flex-col gap-1">
                <p className="font-semibold">
                  {tr.from_lot.code} → {tr.partner.name}
                </p>
                <p className="type-note tabular" style={{ color: 'var(--ink-2)' }}>
                  {tr.quantity} · {t(`supply.status.${tr.status}`)} · {formatTimestamp(tr.dispatched_at)}
                  {tr.received_quantity !== null && ` · ${t('supply.received', { quantity: tr.received_quantity })}`}
                </p>
              </Card>
            </li>
          ))}
        </ul>
      )}

      {confirming && (
        <ConfirmDialog
          title={t('supply.sendTitle')}
          detail={t('supply.sendDetail', {
            quantity: input.quantity,
            lot: options.data?.lots.find((l) => l.id === input.lotId)?.code ?? '',
            partner: live.find((l) => l.id === input.linkId)?.partner.name ?? '',
          })}
          confirmLabel={t('supply.send')}
          cancelLabel={t('supply.cancel')}
          confirming={dispatch.isPending}
          onConfirm={confirm}
          onCancel={() => setConfirming(false)}
        />
      )}
    </section>
  )
}
