import { useRef, useState, type FormEvent } from 'react'
import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { ConfirmDialog } from '@/components/ConfirmDialog'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { FormField } from '@/components/FormField'
import { CONTROL } from '@/components/controlStyles'
import { Card, Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
import { useCrops } from '@/features/officer/useCrops'
import { amountToMinor } from '@/features/ops/surveys/figures'
import { useCurrencies } from '@/features/ops/surveys/useSurveyAdmin'
import { useBatchOptions, useCreateBatch, useCreateRawProduct } from '@/features/ops/supply/useSupply'
import { useVillageCapacity } from '@/features/ops/useOpsReference'
import { fieldErrorsOf } from '@/lib/apiError'
import { errorText } from '@/lib/errors'
import { newUuid } from '@/lib/ids'

/**
 * Buy verified harvests of one village and crop into a raw lot (a supply batch). The API says
 * which harvests and products a batch accepts, hints which product suits the crop, and checks
 * everything again on create. When no product suits the crop, a guided step offers to add one:
 * pre-filled, reviewed and confirmed by a person, never made on its own.
 */
export function BuyHarvestCard() {
  const { t } = useTranslation()
  const villages = useVillageCapacity()
  const { crops, ...cropQuery } = useCrops()
  const currencies = useCurrencies()
  const createBatch = useCreateBatch()
  const createRaw = useCreateRawProduct()

  const [villageId, setVillageId] = useState('')
  const [cropId, setCropId] = useState('')
  const [productId, setProductId] = useState('')
  const [code, setCode] = useState('')
  const [price, setPrice] = useState('')
  const [currencyChoice, setCurrencyChoice] = useState('')
  /** Kg typed per harvest; a harvest not typed yet starts at its kg left. */
  const [kg, setKg] = useState<Record<string, string>>({})
  const [confirming, setConfirming] = useState(false)
  const [rawName, setRawName] = useState<string | null>(null)
  /** Kept until a batch is made, edits included: a resend after a lost reply replays instead of buying twice. */
  const clientRef = useRef(newUuid())
  const sending = useRef(false)

  const options = useBatchOptions(villageId, cropId)
  const crop = crops.find((c) => c.id === cropId)
  const currency = (currencies.data ?? []).find((c) => c.id === currencyChoice)
    ?? (currencies.data ?? []).find((c) => c.default_currency)
    ?? currencies.data?.[0]
  const harvests = options.data?.harvests ?? []
  const lines = harvests
    .map((h) => ({ harvest_report_id: h.harvest_report_id, kg: (kg[h.harvest_report_id] ?? h.available_kg).trim() }))
    .filter((l) => l.kg !== '')
  const pricePerKg = currency ? amountToMinor(price, currency.decimals) : null
  const ready = productId !== '' && code.trim() !== '' && pricePerKg !== null && pricePerKg > 0 && lines.length > 0

  // The API's refusal of a field shows under it; an edit clears it.
  const refused = fieldErrorsOf(createBatch.error, (key) => {
    const line = /^lines\.(\d+)\./.exec(key)
    if (line) return lines[Number(line[1])] ? `batch-kg-${lines[Number(line[1])]!.harvest_report_id}` : null
    return BATCH_INPUT[key] ?? null
  })
  const fieldError = (id: string) => (refused.fields[id] ? errorText(refused.fields[id], t) : undefined)
  const edited = () => {
    if (createBatch.isError || createBatch.isSuccess) createBatch.reset()
  }

  const confirm = () => {
    if (sending.current || !currency || pricePerKg === null) return
    sending.current = true
    createBatch.mutate(
      {
        client_ref: clientRef.current,
        village_id: villageId,
        crop_id: cropId,
        code: code.trim(),
        raw_product_id: productId,
        price_per_kg: pricePerKg,
        currency_id: currency.id,
        lines,
      },
      {
        onSuccess: () => {
          clientRef.current = newUuid()
          setKg({})
        },
        onSettled: () => {
          sending.current = false
          setConfirming(false)
        },
      },
    )
  }

  const rawRefused = fieldErrorsOf(createRaw.error, ['name'])
  const createRawProduct = () => {
    if (rawName === null || createRaw.isPending) return
    createRaw.mutate(rawName, {
      onSuccess: (made) => {
        setProductId(made.id)
        setRawName(null)
        edited()
      },
    })
  }

  const loadError = villages.error ?? cropQuery.error ?? currencies.error
  const noneSuggested = options.data != null && !options.data.raw_products.some((p) => p.suggested_for_crop)

  const select = (id: string, label: string, value: string, onChange: (v: string) => void, rows: { id: string; label: string }[]) => (
    <FormField label={label} id={id} error={fieldError(id)} errorTestId={`${id}-error`}>
      <select
        id={id}
        data-testid={id}
        value={value}
        onChange={(e) => {
          onChange(e.target.value)
          edited()
        }}
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
    <>
      <h2 className="font-semibold">{t('supply.buyTitle')}</h2>
      {loadError ? (
        <ErrorState error={loadError} />
      ) : villages.isLoading || cropQuery.isLoading || currencies.isLoading ? (
        <Loading testId="batch-loading" />
      ) : (
        <Card>
          <form
            data-testid="batch-form"
            className="grid gap-3 sm:grid-cols-2"
            onSubmit={(e: FormEvent) => {
              e.preventDefault()
              if (ready) setConfirming(true)
            }}
          >
            {select('batch-village', t('supply.village'), villageId, (v) => {
              setVillageId(v)
              setKg({})
            }, (villages.data ?? []).map((v) => ({ id: v.id, label: v.name })))}
            {select('batch-crop', t('supply.crop'), cropId, (v) => {
              setCropId(v)
              setProductId('')
              setKg({})
            }, crops.map((c) => ({ id: c.id, label: c.name })))}

            {options.error ? (
              <div className="sm:col-span-2">
                <ErrorState error={options.error} onRetry={() => void options.refetch()} />
              </div>
            ) : options.isLoading ? (
              <div className="sm:col-span-2">
                <Loading testId="batch-options-loading" />
              </div>
            ) : options.data ? (
              <>
                {noneSuggested && (
                  <div data-testid="batch-no-raw-product" className="sm:col-span-2">
                    <Notice
                      variant="info"
                      title={t('supply.noRawTitle', { crop: crop?.name ?? '' })}
                      action={
                        options.data.can_create_product ? (
                          <Button
                            type="button"
                            variant="secondary"
                            data-testid="batch-create-raw"
                            onClick={() => {
                              createRaw.reset()
                              setRawName(t('supply.rawDefault', { crop: crop?.name ?? '' }))
                            }}
                          >
                            {t('supply.createRaw')}
                          </Button>
                        ) : undefined
                      }
                    >
                      {options.data.can_create_product ? t('supply.noRawDetail') : t('supply.noRawAsk')}
                    </Notice>
                  </div>
                )}
                {select(
                  'batch-product',
                  t('supply.product'),
                  productId,
                  setProductId,
                  options.data.raw_products.map((p) => ({ id: p.id, label: p.unit ? `${p.name} (${p.unit})` : p.name })),
                )}
                <FormField label={t('supply.code')} id="batch-code" error={fieldError('batch-code')} errorTestId="batch-code-error">
                  <input
                    id="batch-code"
                    data-testid="batch-code"
                    value={code}
                    onChange={(e) => {
                      setCode(e.target.value)
                      edited()
                    }}
                    className="w-full"
                    style={CONTROL}
                    autoComplete="off"
                  />
                </FormField>
                <FormField label={t('supply.price')} id="batch-price" error={fieldError('batch-price')} errorTestId="batch-price-error">
                  <input
                    id="batch-price"
                    data-testid="batch-price"
                    inputMode="decimal"
                    value={price}
                    onChange={(e) => {
                      setPrice(e.target.value)
                      edited()
                    }}
                    className="w-full"
                    style={CONTROL}
                    autoComplete="off"
                  />
                </FormField>
                {select('batch-currency', t('supply.currency'), currency?.id ?? '', setCurrencyChoice, (currencies.data ?? []).map((c) => ({ id: c.id, label: c.name })))}

                <div className="flex flex-col gap-2 sm:col-span-2">
                  <p className="type-note" style={{ color: 'var(--ink-2)' }}>
                    {t('supply.lines')}
                  </p>
                  {harvests.length === 0 ? (
                    <div data-testid="batch-no-harvests">
                      <EmptyState title={t('supply.noHarvestsTitle')} detail={t('supply.noHarvestsDetail')} />
                    </div>
                  ) : (
                    <ul className="flex flex-col gap-2">
                      {harvests.map((h) => {
                        const id = `batch-kg-${h.harvest_report_id}`
                        return (
                          <li key={h.harvest_report_id} data-testid={`batch-line-${h.harvest_report_id}`}>
                            <FormField
                              label={t('supply.lineRow', { farmer: h.farmer_name, plot: h.plot_label, available: h.available_kg })}
                              id={id}
                              error={fieldError(id)}
                              errorTestId={`${id}-error`}
                            >
                              <input
                                id={id}
                                data-testid={id}
                                inputMode="decimal"
                                value={kg[h.harvest_report_id] ?? h.available_kg}
                                onChange={(e) => {
                                  setKg((now) => ({ ...now, [h.harvest_report_id]: e.target.value }))
                                  edited()
                                }}
                                className="w-full"
                                style={CONTROL}
                                autoComplete="off"
                              />
                            </FormField>
                          </li>
                        )
                      })}
                    </ul>
                  )}
                </div>
              </>
            ) : null}

            {createBatch.error && refused.general != null ? (
              <div data-testid="batch-error" className="sm:col-span-2">
                <ErrorState error={refused.general} />
              </div>
            ) : createBatch.isSuccess ? (
              <div data-testid="batch-made" className="sm:col-span-2">
                <Notice variant="success" title={t('supply.bought', { code: createBatch.data?.lot.code ?? '' })} />
              </div>
            ) : null}
            <div className="sm:col-span-2">
              <Button type="submit" data-testid="batch-submit" disabled={!ready || createBatch.isPending}>
                {createBatch.isPending ? t('supply.buying') : t('supply.buy')}
              </Button>
            </div>
          </form>
        </Card>
      )}

      {confirming && (
        <ConfirmDialog
          title={t('supply.buyConfirmTitle')}
          detail={t('supply.buyConfirmDetail', { count: lines.length, code: code.trim(), price, currency: currency?.name ?? '' })}
          confirmLabel={t('supply.buy')}
          cancelLabel={t('supply.cancel')}
          confirming={createBatch.isPending}
          onConfirm={confirm}
          onCancel={() => setConfirming(false)}
        />
      )}

      {rawName !== null && (
        <ConfirmDialog
          title={t('supply.rawTitle', { crop: crop?.name ?? '' })}
          detail={t('supply.rawDetail')}
          confirmLabel={t('supply.rawCreate')}
          cancelLabel={t('supply.cancel')}
          confirming={createRaw.isPending}
          onConfirm={createRawProduct}
          onCancel={() => setRawName(null)}
        >
          <FormField
            label={t('supply.rawName')}
            id="raw-product-name"
            error={rawRefused.fields.name ? errorText(rawRefused.fields.name, t) : undefined}
            errorTestId="raw-product-name-error"
          >
            <input
              id="raw-product-name"
              data-testid="raw-product-name"
              value={rawName}
              onChange={(e) => setRawName(e.target.value)}
              className="w-full"
              style={CONTROL}
              autoComplete="off"
            />
          </FormField>
          {createRaw.error && rawRefused.general != null ? <ErrorState error={rawRefused.general} /> : null}
        </ConfirmDialog>
      )}
    </>
  )
}

/** The input each API key belongs to (lines map to their harvest's kg input). */
const BATCH_INPUT: Record<string, string> = {
  village_id: 'batch-village',
  crop_id: 'batch-crop',
  raw_product_id: 'batch-product',
  code: 'batch-code',
  price_per_kg: 'batch-price',
  currency_id: 'batch-currency',
}
