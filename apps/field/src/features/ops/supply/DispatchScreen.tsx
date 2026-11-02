import { useRef, useState, type FormEvent } from 'react'
import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { fieldErrorsOf } from '@/lib/apiError'
import { errorText } from '@/lib/errors'
import { FormField } from '@/components/FormField'
import { PageHeader } from '@/components/PageHeader'
import { CONTROL } from '@/components/controlStyles'
import { Card, Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
import {
  type DispatchInput,
  useDispatch,
  useDispatchOptions,
  useLotStock,
  useSupplyLinks,
  useSupplyTransfers,
} from '@/features/ops/supply/useSupply'
import { formatTimestamp } from '@/lib/format'
import { newUuid } from '@/lib/ids'

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
  /**
   * Kept until a transfer is sent, edits included: if a send landed but its reply was lost, a resend with a
   * changed body is refused (422) instead of moving the stock twice.
   */
  const idempotencyKey = useRef(newUuid())
  const stock = useLotStock(input.lotId)

  const live = (links.data ?? []).filter((l) => l.status === 'live')
  const warehouseName = (id: string) => options.data?.warehouses.find((w) => w.id === id)?.name ?? id
  // The API's refusal of a field is shown under it; an edit clears it, and a shown success.
  const refused = fieldErrorsOf(dispatch.error, (key) => DISPATCH_INPUT[key])
  const fieldError = (id: string) => (refused.fields[id] ? errorText(refused.fields[id], t) : undefined)
  const set = (key: keyof DispatchInput) => (e: { target: { value: string } }) => {
    setInput((now) => ({ ...now, [key]: e.target.value }))
    if (dispatch.isError || dispatch.isSuccess) dispatch.reset()
  }
  const ready = Object.values(input).every((v) => v.trim() !== '')

  const confirm = () => {
    if (sending.current) return
    sending.current = true
    dispatch.mutate({ ...input, idempotencyKey: idempotencyKey.current }, {
      onSuccess: () => {
        idempotencyKey.current = newUuid()
        setInput((now) => ({ ...now, quantity: '' }))
      },
      onSettled: () => {
        sending.current = false
        setConfirming(false)
      },
    })
  }

  const loadError = links.error ?? options.error
  const select = (key: keyof DispatchInput, id: string, label: string, rows: { id: string; label: string }[]) => (
    <FormField label={label} id={id} error={fieldError(id)} errorTestId={`${id}-error`}>
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
            {input.lotId !== '' && (
              // Display only: the API still refuses a quantity above what is free.
              <div data-testid="dispatch-stock" className="flex flex-col gap-1 sm:col-span-2">
                <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                  {t('supply.stockTitle')}
                </p>
                {stock.error ? (
                  <ErrorState error={stock.error} />
                ) : stock.isLoading ? (
                  <Loading testId="dispatch-stock-loading" />
                ) : !stock.data?.length ? (
                  <p className="type-note">{t('supply.noStock')}</p>
                ) : (
                  <ul className="flex flex-col gap-0.5">
                    {stock.data.map((row) => (
                      <li key={row.warehouse_id} data-testid="dispatch-stock-row" className="type-note tabular">
                        <b>{warehouseName(row.warehouse_id)}</b>
                        {': '}
                        {t('supply.stockRow', { available: row.available, level: row.level, reserved: row.reserved })}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            )}
            <FormField label={t('supply.quantity')} id="dispatch-quantity" error={fieldError('dispatch-quantity')} errorTestId="dispatch-quantity-error">
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
            {dispatch.error && refused.general != null ? (
              <div data-testid="dispatch-error" className="sm:col-span-2">
                <ErrorState error={refused.general} />
              </div>
            ) : dispatch.isSuccess ? (
              <div data-testid="dispatch-sent" className="sm:col-span-2">
                <Notice variant="success" title={t('supply.sent')} />
              </div>
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

/** The input each API key belongs to. */
const DISPATCH_INPUT: Record<string, string> = { link_id: 'dispatch-link', warehouse_id: 'dispatch-warehouse', quantity: 'dispatch-quantity' }
