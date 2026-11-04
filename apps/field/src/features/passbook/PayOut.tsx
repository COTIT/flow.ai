import { useRef, useState } from 'react'
import type { Money } from '@genesy/api-client/model'
import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { ErrorState } from '@/components/ErrorState'
import { FormField } from '@/components/FormField'
import { CONTROL } from '@/components/controlStyles'
import { Button } from '@/components/ui/button'
import { amountToMinor } from '@/features/ops/surveys/figures'
import { useCreatePayout } from '@/features/passbook/usePassbook'
import { fieldErrorsOf } from '@/lib/apiError'
import { errorText } from '@/lib/errors'
import { formatPrice } from '@/lib/format'
import { newUuid } from '@/lib/ids'

const ID_TYPES = ['nida', 'voter', 'driving_licence', 'village_letter'] as const

/**
 * Staff pay a farmer cash from one currency's balance. The API decides everything: the balance
 * (422 on amount), four eyes and the officer limit (403, its reason shown as sent).
 */
export function PayOut({ personId, personName, balance }: { personId: string; personName: string; balance: Money }) {
  const { t, i18n } = useTranslation()
  const payout = useCreatePayout()
  const [open, setOpen] = useState(false)
  const [amount, setAmount] = useState('')
  const [idType, setIdType] = useState('')
  const [nameConfirmed, setNameConfirmed] = useState(false)
  /** Kept until a payout is made, edits included: a resend after a lost reply replays instead of paying twice. */
  const clientRef = useRef(newUuid())
  const sending = useRef(false)
  const money = (m: Money) => formatPrice(m, i18n.resolvedLanguage ?? 'en')

  const minor = amountToMinor(amount, balance.decimals)
  const ready = minor !== null && minor > 0 && idType !== '' && nameConfirmed
  const refused = fieldErrorsOf(payout.error, ['amount'])

  const pay = () => {
    if (sending.current || !ready) return
    sending.current = true
    payout.mutate(
      { client_ref: clientRef.current, person_id: personId, amount: minor, currency_id: balance.currency_id, id_type: idType, name_confirmed: true },
      {
        onSuccess: () => {
          clientRef.current = newUuid()
          setOpen(false)
          setAmount('')
          setIdType('')
          setNameConfirmed(false)
        },
        onSettled: () => {
          sending.current = false
        },
      },
    )
  }

  return (
    <>
      <Button
        type="button"
        variant="secondary"
        data-testid={`pay-out-${balance.currency}`}
        onClick={() => {
          payout.reset()
          setOpen(true)
        }}
      >
        {t('passbook.payOut')}
      </Button>
      {payout.isSuccess && payout.data && !open ? (
        <div data-testid="payout-done">
          <Notice variant="success" title={t('passbook.paid', { amount: money(payout.data.amount) })} />
        </div>
      ) : null}

      {open && (
        <ConfirmDialog
          title={t('passbook.payTitle', { name: personName })}
          detail={t('passbook.payDetail', { balance: money(balance) })}
          confirmLabel={payout.isPending ? t('passbook.paying') : t('passbook.pay')}
          cancelLabel={t('passbook.cancel')}
          confirming={payout.isPending}
          confirmDisabled={!ready}
          onConfirm={pay}
          onCancel={() => setOpen(false)}
        >
          <FormField
            label={t('passbook.amount', { currency: balance.currency })}
            id="payout-amount"
            error={refused.fields.amount ? errorText(refused.fields.amount, t) : undefined}
            errorTestId="payout-amount-error"
          >
            <input
              id="payout-amount"
              data-testid="payout-amount"
              inputMode="decimal"
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              className="w-full"
              style={CONTROL}
              autoComplete="off"
            />
          </FormField>
          <FormField label={t('passbook.idTypeLabel')} id="payout-id-type">
            <select
              id="payout-id-type"
              data-testid="payout-id-type"
              value={idType}
              onChange={(e) => setIdType(e.target.value)}
              className="w-full"
              style={CONTROL}
            >
              <option value="">{t('passbook.choose')}</option>
              {ID_TYPES.map((k) => (
                <option key={k} value={k}>
                  {t(`passbook.idType.${k}`)}
                </option>
              ))}
            </select>
          </FormField>
          <label className="flex items-center gap-2.5" style={{ minHeight: 44, fontSize: 15 }}>
            <input
              type="checkbox"
              data-testid="payout-name-confirmed"
              checked={nameConfirmed}
              onChange={(e) => setNameConfirmed(e.target.checked)}
              style={{ width: 20, height: 20 }}
            />
            {t('passbook.nameConfirmed')}
          </label>
          {payout.error && refused.general != null ? (
            <div data-testid="payout-error">
              <ErrorState error={refused.general} />
            </div>
          ) : null}
        </ConfirmDialog>
      )}
    </>
  )
}
