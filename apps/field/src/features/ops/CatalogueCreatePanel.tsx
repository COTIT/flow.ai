import { useRef, useState, type ReactNode } from 'react'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
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
import { finishDraftWhenSaved } from '@/lib/drafts'
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

const numberOrNull = (value: string) => (value.trim() === '' ? null : Number(value.trim()))
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
    'buy-price': numberError(price),
    'rent-price': numberError(rent),
  }
  const shown = (key: keyof typeof errors) => (touched && errors[key] ? t(errors[key]!) : undefined)

  const submit = () => {
    if (inFlight.current || create.isPending || !draft.ready) return
    setTouched(true)
    if (Object.values(errors).some(Boolean) || !projectId) return

    inFlight.current = true
    void finishDraftWhenSaved(create.mutateAsync({
      id: draft.clientRef,
      project_id: projectId,
      category_id: categoryId,
      code: code.trim(),
      name_en: nameEn.trim(),
      name_sw: nameSw.trim(),
      rated_power_kw: numberOrNull(power),
      typical_hours_per_day: numberOrNull(hours),
      typical_days_per_week: numberOrNull(days),
      can_rent: canRent === 'true',
      can_buy: canBuy === 'true',
      indicative_price: numberOrNull(price),
      indicative_rent_per_day: numberOrNull(rent),
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
    <CreateForm onSubmit={submit} dirty={draft.dirty} storageError={draft.storageError} create={create} ready={draft.ready}>
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
    min: numberError(min, true),
    max: numberError(max, true),
  }
  const shown = (key: keyof typeof errors) => (touched && errors[key] ? t(errors[key]!) : undefined)

  const submit = () => {
    if (inFlight.current || create.isPending || !draft.ready) return
    setTouched(true)
    if (Object.values(errors).some(Boolean) || !projectId) return

    inFlight.current = true
    void finishDraftWhenSaved(create.mutateAsync({
      id: draft.clientRef,
      project_id: projectId,
      code: code.trim(),
      name_en: nameEn.trim(),
      name_sw: nameSw.trim(),
      description_en: textOrNull(descriptionEn),
      description_sw: textOrNull(descriptionSw),
      indicative_min_amount: Number(min.trim()),
      indicative_max_amount: Number(max.trim()),
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
    <CreateForm onSubmit={submit} dirty={draft.dirty} storageError={draft.storageError} create={create} ready={draft.ready}
      note={t('resources.loanNote')}
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
      <Field label={t('catalogue.descriptionEn')} id="catalogue-description-en">
        {input('description-en', descriptionEn, setDescriptionEn)}
      </Field>
      <Field label={t('catalogue.descriptionSw')} id="catalogue-description-sw">
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
  dirty,
  storageError,
  ready,
  create,
  note,
  children,
}: {
  onSubmit: () => void
  dirty: boolean
  storageError: boolean
  ready: boolean
  create: { isPending: boolean; isError: boolean; error: unknown; reset: () => void }
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
      {note && <p className="type-note" style={{ color: 'var(--ink-2)', textWrap: 'pretty' }}>{note}</p>}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">{children}</div>
      <p className="type-note" style={{ color: 'var(--ink-3)' }}>{t('catalogue.indicativeNote')}</p>
      {create.isError && (
        <div data-testid="catalogue-create-error">
          <ErrorState error={create.error} onRetry={() => create.reset()} />
        </div>
      )}
      <Button type="submit" data-testid="catalogue-create-submit" disabled={!ready || create.isPending} className="w-fit">
        {create.isPending ? t('catalogue.adding') : t('catalogue.addSubmit')}
      </Button>
    </form>
  )
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
