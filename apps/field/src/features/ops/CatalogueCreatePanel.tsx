import { useRef, useState, type ReactNode } from 'react'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { Notice } from '@genesy/ui/components/notice'
import { useTranslation } from 'react-i18next'

import { useSession } from '@/app/session'
import { ErrorState } from '@/components/ErrorState'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { FormField as Field } from '@/components/FormField'
import { CONTROL } from '@/components/controlStyles'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import type { ResourceKind } from '@/features/farmer/resourceKind'
import {
  useCreateEquipment,
  useCreateLoanProduct,
  useEquipmentCategories,
} from '@/features/ops/useCatalogue'
import { amountToMinor } from '@/features/ops/surveys/figures'
import { useCurrencies } from '@/features/ops/surveys/useSurveyAdmin'
import { firstFailure } from '@/lib/apiError'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { useFieldErrors } from '@/lib/useFieldErrors'
import { usePersistentForm } from '@/lib/usePersistentForm'

/**
 * Add a row to the resource catalogue — just enough of a form, not a full
 * editor. Equipment or a loan listing, by the tab that is open.
 *
 * What is checked here is only what a form owns: a required box is not empty,
 * a number is a number (`Number('12,5')` is NaN and would be sent as null,
 * dropping what was typed). Everything else — offered to rent or buy, a range
 * that runs forwards, a unique code — is the database's, and its message is
 * shown.
 */
export function CatalogueCreatePanel({ kind, onClose }: { kind: ResourceKind; onClose: () => void }) {
  const { t } = useTranslation()
  const session = useSession()
  const projectId = session.data?.memberships.find((m) => m.role === 'ops' || m.role === 'admin')?.project_id

  return (
    <section
      id="catalogue-create-panel"
      data-testid="catalogue-create-panel"
      className="flex w-full flex-col gap-3 p-4 sm:p-[18px]"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>{t(`catalogue.createTitle.${kind}`)}</h2>
        <Button type="button" data-testid="catalogue-create-close" variant="ghost" size="sm" onClick={onClose}>
          {t('common.close')}
        </Button>
      </div>
      {kind === 'equipment' ? <EquipmentForm projectId={projectId} /> : <LoanForm projectId={projectId} />}
    </section>
  )
}

/** Required text. Returns an i18n key, or undefined when fine. */
const textError = (value: string) => (value.trim() === '' ? 'catalogue.required' : undefined)

/** A number's shape only. Blank is fine unless required. */
function numberError(value: string, required = false): string | undefined {
  const text = value.trim()
  if (text === '') return required ? 'catalogue.required' : undefined
  return Number.isFinite(Number(text)) ? undefined : 'catalogue.notANumber'
}

const textOrNull = (value: string) => (value.trim() === '' ? null : value.trim())

const equipmentSchema = z.object({
  code: z.string(), nameEn: z.string(), nameSw: z.string(), categoryId: z.string(),
  power: z.string(), hours: z.string(), days: z.string(),
  canRent: z.string(), canBuy: z.string(), price: z.string(), rent: z.string(),
})
const EQUIPMENT_DEFAULTS = {
  code: '', nameEn: '', nameSw: '', categoryId: '', power: '', hours: '', days: '',
  canRent: 'false', canBuy: 'true', price: '', rent: '',
}

function EquipmentForm({ projectId }: { projectId: string | undefined }) {
  const { t } = useTranslation()
  const create = useCreateEquipment()
  const categories = useEquipmentCategories()
  // shortcut: prices are in the account's default currency (the labels say TZS); add a currency
  // picker when an account prices equipment in more than one.
  const currencyQuery = useCurrencies()
  const currencies = currencyQuery.data ?? []
  const currency = currencies.find((c) => c.default_currency) ?? currencies[0]
  /** Integer minor units of the currency, by string maths; null when blank or not an amount. */
  const minor = (text: string) => (text.trim() === '' ? null : amountToMinor(text.trim(), currency?.decimals ?? 0))
  const priceError = (text: string) =>
    numberError(text) ?? (text.trim() !== '' && minor(text) === null ? 'catalogue.notANumber' : undefined)
  const draft = usePersistentForm('catalogue-equipment-create', projectId ?? 'project', EQUIPMENT_DEFAULTS, zodResolver(equipmentSchema))
  const [code, setCode] = draft.field('code')
  const [nameEn, setNameEn] = draft.field('nameEn')
  const [nameSw, setNameSw] = draft.field('nameSw')
  const [categoryId, setCategoryId] = draft.field('categoryId')
  const [power, setPower] = draft.field('power')
  const [hours, setHours] = draft.field('hours')
  const [days, setDays] = draft.field('days')
  const [canRent, setCanRent] = draft.field('canRent')
  const [canBuy, setCanBuy] = draft.field('canBuy')
  const [price, setPrice] = draft.field('price')
  const [rent, setRent] = draft.field('rent')
  const [touched, setTouched] = useState(false)
  const inFlight = useRef(false)

  const errors = {
    code: textError(code),
    'name-en': textError(nameEn),
    'name-sw': textError(nameSw),
    category: categoryId === '' ? 'catalogue.required' : undefined,
    power: numberError(power),
    hours: numberError(hours),
    days: numberError(days),
    'buy-price': priceError(price),
    'rent-price': priceError(rent),
  }
  // The API's refusal of a field, as written, under that field; the form's own check wins while it fails.
  const refusal = useFieldErrors(create.error, draft.setError, (key) => EQUIPMENT_FIELD[key], (field) => `catalogue-${EQUIPMENT_INPUT[field]}`)
  const shown = (key: keyof typeof errors) =>
    (touched && errors[key] ? t(errors[key]!) : undefined) ?? serverError(draft.formState.errors, EQUIPMENT_INPUT, key)

  const submit = () => {
    if (inFlight.current || create.isPending || !draft.ready) return
    setTouched(true)
    if (Object.values(errors).some(Boolean) || !currency) return

    inFlight.current = true
    // Figures go as the decimal text typed, money as integer minor units: no floats.
    void finishDraftWhenSaved(create.mutateAsync({
      id: draft.clientRef,
      category_id: categoryId,
      code: code.trim(),
      name_en: nameEn.trim(),
      name_sw: nameSw.trim(),
      rated_power_kw: textOrNull(power),
      typical_hours_per_day: textOrNull(hours),
      typical_days_per_week: textOrNull(days),
      can_rent: canRent === 'true',
      can_buy: canBuy === 'true',
      indicative_price_minor: minor(price),
      indicative_rent_per_day_minor: minor(rent),
      currency_id: currency.id,
    }), draft.finish).finally(() => { inFlight.current = false })
  }

  const input = (id: keyof typeof errors, value: string, set: (v: string) => void, numeric = false) => (
    <input
      id={`catalogue-${id}`}
      data-testid={`catalogue-${id}`}
      inputMode={numeric ? 'decimal' : undefined}
      value={value}
      onChange={(e) => set(e.target.value)}
      className="w-full"
      style={CONTROL}
    />
  )

  return (
    <CreateForm onSubmit={submit} loadFailure={firstFailure([categories, currencyQuery])} dirty={draft.dirty} storageError={draft.storageError} create={create} refusal={refusal} saved={create.isSuccess && !draft.dirty && t('catalogue.equipmentCreated')} onRetry={() => void finishDraftWhenSaved(create.mutateAsync(create.variables!), draft.finish)} ready={draft.ready && Boolean(currency)}>
      <Field label={t('catalogue.code')} id="catalogue-code" error={shown('code')} errorTestId="catalogue-code-error">
        {input('code', code, setCode)}
      </Field>
      <Field label={t('catalogue.nameEn')} id="catalogue-name-en" error={shown('name-en')} errorTestId="catalogue-name-en-error">
        {input('name-en', nameEn, setNameEn)}
      </Field>
      <Field label={t('catalogue.nameSw')} id="catalogue-name-sw" error={shown('name-sw')} errorTestId="catalogue-name-sw-error">
        {input('name-sw', nameSw, setNameSw)}
      </Field>
      <Field label={t('catalogue.colCategory')} id="catalogue-category" error={shown('category')} errorTestId="catalogue-category-error">
        <Select value={categoryId} onValueChange={(value) => setCategoryId(value ?? '')}>
          <SelectTrigger id="catalogue-category" data-testid="catalogue-category" className="w-full">
            {categories.data?.find((c) => c.id === categoryId)?.name ?? t('catalogue.chooseCategory')}
          </SelectTrigger>
          <SelectContent>
            {(categories.data ?? []).map((c) => <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>)}
          </SelectContent>
        </Select>
      </Field>
      <Field label={t('catalogue.colPower')} id="catalogue-power" error={shown('power')} errorTestId="catalogue-power-error">
        {input('power', power, setPower, true)}
      </Field>
      <Field label={t('catalogue.colHours')} id="catalogue-hours" error={shown('hours')} errorTestId="catalogue-hours-error">
        {input('hours', hours, setHours, true)}
      </Field>
      <Field label={t('catalogue.colDays')} id="catalogue-days" error={shown('days')} errorTestId="catalogue-days-error">
        {input('days', days, setDays, true)}
      </Field>
      <fieldset className="flex min-w-0 flex-col gap-1.5">
        <legend style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>{t('catalogue.colOffered')}</legend>
        <div className="flex flex-wrap gap-4" style={{ minHeight: 44, alignItems: 'center' }}>
          <Check testId="catalogue-can-rent" label={t('resources.offered.rent')} checked={canRent === 'true'} onChange={(on) => setCanRent(String(on))} />
          <Check testId="catalogue-can-buy" label={t('resources.offered.buy')} checked={canBuy === 'true'} onChange={(on) => setCanBuy(String(on))} />
        </div>
      </fieldset>
      <Field label={t('catalogue.priceInput')} id="catalogue-buy-price" error={shown('buy-price')} errorTestId="catalogue-buy-price-error">
        {input('buy-price', price, setPrice, true)}
      </Field>
      <Field label={t('catalogue.rentInput')} id="catalogue-rent-price" error={shown('rent-price')} errorTestId="catalogue-rent-price-error">
        {input('rent-price', rent, setRent, true)}
      </Field>
    </CreateForm>
  )
}

const loanSchema = z.object({
  code: z.string(), nameEn: z.string(), nameSw: z.string(),
  descriptionEn: z.string(), descriptionSw: z.string(), min: z.string(), max: z.string(),
})
const LOAN_DEFAULTS = { code: '', nameEn: '', nameSw: '', descriptionEn: '', descriptionSw: '', min: '', max: '' }

/**
 * A loan LISTING: a name, a description and an indicative range — and
 * deliberately nothing else. Research item C is open, so no interest,
 * deposit, term or repayment field exists to fill in.
 */
function LoanForm({ projectId }: { projectId: string | undefined }) {
  const { t } = useTranslation()
  const create = useCreateLoanProduct()
  // shortcut: amounts are in the account's default currency, as for equipment; add a currency
  // picker when an account lists loans in more than one.
  const currencyQuery = useCurrencies()
  const currencies = currencyQuery.data ?? []
  const currency = currencies.find((c) => c.default_currency) ?? currencies[0]
  const minor = (text: string) => amountToMinor(text.trim(), currency?.decimals ?? 0)
  const amountError = (text: string) =>
    numberError(text, true) ?? (minor(text) === null ? 'catalogue.notANumber' : undefined)
  const draft = usePersistentForm('catalogue-loan-create', projectId ?? 'project', LOAN_DEFAULTS, zodResolver(loanSchema))
  const [code, setCode] = draft.field('code')
  const [nameEn, setNameEn] = draft.field('nameEn')
  const [nameSw, setNameSw] = draft.field('nameSw')
  const [descriptionEn, setDescriptionEn] = draft.field('descriptionEn')
  const [descriptionSw, setDescriptionSw] = draft.field('descriptionSw')
  const [min, setMin] = draft.field('min')
  const [max, setMax] = draft.field('max')
  const [touched, setTouched] = useState(false)
  const inFlight = useRef(false)

  const errors = {
    code: textError(code),
    'name-en': textError(nameEn),
    'name-sw': textError(nameSw),
    min: amountError(min),
    max: amountError(max),
  }
  const refusal = useFieldErrors(create.error, draft.setError, (key) => LOAN_FIELD[key], (field) => `catalogue-${LOAN_INPUT[field]}`)
  const server = (key: string) => serverError(draft.formState.errors, LOAN_INPUT, key)
  const shown = (key: keyof typeof errors) => (touched && errors[key] ? t(errors[key]!) : undefined) ?? server(key)

  const submit = () => {
    if (inFlight.current || create.isPending || !draft.ready) return
    setTouched(true)
    if (Object.values(errors).some(Boolean) || !currency) return

    inFlight.current = true
    // Amounts as integer minor units by string maths: no floats. The range rule is the API's.
    void finishDraftWhenSaved(create.mutateAsync({
      id: draft.clientRef,
      code: code.trim(),
      name_en: nameEn.trim(),
      name_sw: nameSw.trim(),
      description_en: textOrNull(descriptionEn),
      description_sw: textOrNull(descriptionSw),
      indicative_min_minor: minor(min)!,
      indicative_max_minor: minor(max)!,
      currency_id: currency.id,
    }), draft.finish).finally(() => { inFlight.current = false })
  }

  const input = (id: string, value: string, set: (v: string) => void, numeric = false) => (
    <input
      id={`catalogue-${id}`}
      data-testid={`catalogue-${id}`}
      inputMode={numeric ? 'decimal' : undefined}
      value={value}
      onChange={(e) => set(e.target.value)}
      className="w-full"
      style={CONTROL}
    />
  )

  return (
    <CreateForm onSubmit={submit} loadFailure={firstFailure([currencyQuery])} dirty={draft.dirty} storageError={draft.storageError} create={create} refusal={refusal} saved={create.isSuccess && !draft.dirty && t('catalogue.loanCreated')}
      onRetry={() => void finishDraftWhenSaved(create.mutateAsync(create.variables!), draft.finish)}
      ready={draft.ready && Boolean(currency)} note={t('resources.loanNote')}
    >
      <Field label={t('catalogue.code')} id="catalogue-code" error={shown('code')} errorTestId="catalogue-code-error">
        {input('code', code, setCode)}
      </Field>
      <Field label={t('catalogue.nameEn')} id="catalogue-name-en" error={shown('name-en')} errorTestId="catalogue-name-en-error">
        {input('name-en', nameEn, setNameEn)}
      </Field>
      <Field label={t('catalogue.nameSw')} id="catalogue-name-sw" error={shown('name-sw')} errorTestId="catalogue-name-sw-error">
        {input('name-sw', nameSw, setNameSw)}
      </Field>
      <Field label={t('catalogue.descriptionEn')} id="catalogue-description-en" error={server('description-en')} errorTestId="catalogue-description-en-error">
        {input('description-en', descriptionEn, setDescriptionEn)}
      </Field>
      <Field label={t('catalogue.descriptionSw')} id="catalogue-description-sw" error={server('description-sw')} errorTestId="catalogue-description-sw-error">
        {input('description-sw', descriptionSw, setDescriptionSw)}
      </Field>
      <Field label={t('catalogue.minInput')} id="catalogue-min" error={shown('min')} errorTestId="catalogue-min-error">
        {input('min', min, setMin, true)}
      </Field>
      <Field label={t('catalogue.maxInput')} id="catalogue-max" error={shown('max')} errorTestId="catalogue-max-error">
        {input('max', max, setMax, true)}
      </Field>
    </CreateForm>
  )
}

/** The form frame both kinds share: grid, draft status, error, submit. */
function CreateForm({
  onSubmit,
  loadFailure,
  dirty,
  storageError,
  ready,
  create,
  refusal,
  saved,
  onRetry,
  note,
  children,
}: {
  onSubmit: () => void
  /** A read the form needs (categories, currency) that failed: without it "Add" stays disabled, so say why. */
  loadFailure: { error: unknown; retry: () => void }
  dirty: boolean
  storageError: boolean
  ready: boolean
  create: { isPending: boolean; isError: boolean }
  /** What the general error Notice still has to say once refused fields show their own (null: nothing). */
  refusal: unknown
  /** The success Notice's title, while the saved form is untouched. */
  saved: string | false
  /** Re-sends the save that failed, unchanged: same id, same body. */
  onRetry: () => void
  note?: string
  children: ReactNode
}) {
  const { t } = useTranslation()
  return (
    <form
      className="flex flex-col gap-3"
      noValidate
      onSubmit={(e) => {
        e.preventDefault()
        onSubmit()
      }}
    >
      <FormDraftStatus dirty={dirty} storageError={storageError} />
      {loadFailure.error != null && (
        <div data-testid="catalogue-options-error">
          <ErrorState error={loadFailure.error} onRetry={loadFailure.retry} />
        </div>
      )}
      {note && <p className="type-note" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>{note}</p>}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">{children}</div>
      <p className="type-note" style={{ color: 'var(--ink-3)' }}>{t('catalogue.indicativeNote')}</p>
      {create.isError && refusal != null && (
        <div data-testid="catalogue-create-error">
          <ErrorState error={refusal} onRetry={onRetry} />
        </div>
      )}
      {saved && (
        <div data-testid="catalogue-saved">
          <Notice variant="success" title={saved} />
        </div>
      )}
      <Button type="submit" data-testid="catalogue-create-submit" disabled={!ready || create.isPending} className="w-fit">
        {create.isPending ? t('catalogue.adding') : t('catalogue.addSubmit')}
      </Button>
    </form>
  )
}

/** The form field each API key belongs to, and that field's input (`catalogue-<input>`). */
const EQUIPMENT_FIELD: Record<string, keyof typeof EQUIPMENT_DEFAULTS> = {
  code: 'code', name_en: 'nameEn', name_sw: 'nameSw', category_id: 'categoryId', rated_power_kw: 'power',
  typical_hours_per_day: 'hours', typical_days_per_week: 'days', indicative_price_minor: 'price',
  indicative_rent_per_day_minor: 'rent',
}
const EQUIPMENT_INPUT: Record<string, string> = {
  code: 'code', nameEn: 'name-en', nameSw: 'name-sw', categoryId: 'category', power: 'power', hours: 'hours',
  days: 'days', price: 'buy-price', rent: 'rent-price',
}
const LOAN_FIELD: Record<string, keyof typeof LOAN_DEFAULTS> = {
  code: 'code', name_en: 'nameEn', name_sw: 'nameSw', description_en: 'descriptionEn', description_sw: 'descriptionSw',
  indicative_min_minor: 'min', indicative_max_minor: 'max',
}
const LOAN_INPUT: Record<string, string> = {
  code: 'code', nameEn: 'name-en', nameSw: 'name-sw', descriptionEn: 'description-en', descriptionSw: 'description-sw',
  min: 'min', max: 'max',
}

/** The API's text set on the form field whose input is `input`. */
function serverError(errors: Partial<Record<string, { message?: string }>>, inputs: Record<string, string>, input: string) {
  const field = Object.keys(inputs).find((name) => inputs[name] === input)
  return field ? errors[field]?.message : undefined
}

function Check({
  testId,
  label,
  checked,
  onChange,
}: {
  testId: string
  label: string
  checked: boolean
  onChange: (on: boolean) => void
}) {
  return (
    <label className="inline-flex items-center gap-2" style={{ fontSize: 15, color: 'var(--ink)' }}>
      <input
        type="checkbox"
        data-testid={testId}
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        style={{ width: 20, height: 20, accentColor: '#1d70b7' }}
      />
      {label}
    </label>
  )
}
