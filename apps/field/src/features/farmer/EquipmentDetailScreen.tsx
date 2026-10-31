import { useEffect, useRef, useState } from 'react'
import { Link, getRouteApi } from '@tanstack/react-router'
import { useWatch } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useTranslation } from 'react-i18next'

import { activeMemberships, ownVillageId } from '@/app/membership'
import { useSession } from '@/app/session'
import { EmptyState } from '@/components/EmptyState'
import { EnergyEstimatePanel } from '@/components/EnergyEstimatePanel'
import { ErrorState } from '@/components/ErrorState'
import { IndicativePill, Loading } from '@/components/controls'
import { BangMark } from '@/components/marks'
import { useEquipmentItem } from '@/features/farmer/useEquipment'
import { requestSchema, type RequestForm } from '@/features/farmer/requestSchema'
import { z } from 'zod'
import { useSubmitRequest } from '@/features/farmer/useRequests'
import { formatKw, formatMoney } from '@/lib/format'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { usePersistentForm } from '@/lib/usePersistentForm'
import { FormDraftStatus } from '@/components/FormDraftStatus'

const route = getRouteApi('/_farmer/farm/equipment/$equipmentId')

type Acquisition = 'rent' | 'buy'

/** The request as typed, plus how the farmer wants the machine. */
type RequestDraft = RequestForm & { acquisition: string }

const EMPTY: RequestDraft = {
  quantity: '1',
  hours_per_day: '',
  days_per_week: '',
  purpose: '',
  acquisition: '',
}

/**
 * The persisted form carries `acquisition` beside the estimate's inputs. It is
 * not part of `requestSchema`, which also gates the live estimate: a machine
 * draws the same power rented or owned, so an unanswered choice must not
 * withhold the estimate.
 */
const draftSchema = requestSchema.extend({ acquisition: z.string() })

const isAcquisition = (value: string): value is Acquisition => value === 'rent' || value === 'buy'

/**
 * Spec 6.4 — detail plus the request form, with a live estimate.
 *
 * The estimate shown here is a PREVIEW. pue_recompute_estimate writes the
 * stored row, and the request detail reads that stored figure rather than
 * recomputing — so the two can be compared instead of assumed equal.
 *
 * The inputs are bounded by `requestSchema` (QA #9). The estimate is only
 * computed from inputs that pass it: the finding's real complaint was not that
 * 99 hours was accepted, but that the screen computed a confident
 * 1,485 kWh/day from it and presented that as an answer.
 */
export function EquipmentDetailScreen() {
  const { equipmentId } = route.useParams()
  const { t } = useTranslation()
  const session = useSession()
  const query = useEquipmentItem(equipmentId)
  const submit = useSubmitRequest()

  const draft = usePersistentForm<RequestDraft>('equipment-request', equipmentId, EMPTY, zodResolver(draftSchema))
  const { handleSubmit, control, reset, formState: { errors, isSubmitting } } = draft
  const [quantity, setQuantity] = draft.field('quantity')
  const [hoursPerDay, setHoursPerDay] = draft.field('hours_per_day')
  const [daysPerWeek, setDaysPerWeek] = draft.field('days_per_week')
  const [purpose, setPurpose] = draft.field('purpose')
  const [acquisition, setAcquisition] = draft.field('acquisition')
  const [acquisitionMissing, setAcquisitionMissing] = useState(false)

  const item = query.item

  // Prefilled from the equipment's typicals, once. Overwriting on every render
  // would fight the farmer as they type. A ref rather than state: `reset` is
  // what re-renders, so a state flag would only add a second render and a
  // set-state-in-effect to explain away.
  const prefilled = useRef(false)
  useEffect(() => {
    if (prefilled.current || !item || !draft.ready) return
    prefilled.current = true
    if (!draft.dirty) reset({
      ...EMPTY,
      hours_per_day: item.typical_hours_per_day === null ? '' : String(item.typical_hours_per_day),
      days_per_week: item.typical_days_per_week === null ? '' : String(item.typical_days_per_week),
    })
  }, [item, reset, draft.ready, draft.dirty])

  // Offered one way only: that is the answer, and there is nothing to choose.
  // Whether it is offered that way at all is pue_request_acquisition_check's.
  const offered: Acquisition[] = item
    ? ([item.can_rent && 'rent', item.can_buy && 'buy'].filter(Boolean) as Acquisition[])
    : []
  const only = offered.length === 1 ? offered[0] : undefined
  useEffect(() => {
    if (draft.ready && only && acquisition !== only) setAcquisition(only)
  }, [draft.ready, only, acquisition, setAcquisition])

  // Subscribed so the estimate recalculates live, and so it can be withheld
  // while the assumptions behind it are not possible. `useWatch` rather than
  // `watch()` because the latter returns a fresh object on every render.
  const values = useWatch({ control })
  const parsed = requestSchema.safeParse(values)

  /**
   * QA #32. A withheld estimate has two quite different causes, and telling a
   * farmer to "fill in hours per day" when they have just typed 99 into it
   * asks them to do something they have already done.
   *
   * A blank is the one they can act on first, so it wins when both are true.
   */
  const anythingBlank =
    !parsed.success &&
    parsed.error.issues.some((issue) => issue.message === 'equipment.required')

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading || session.isLoading) {
    return <Loading testId="equipment-detail-loading" />
  }

  // Zero rows is an answer: not listed, or outside this project's scope.
  if (!item) {
    return <EmptyState title={t('equipment.notFoundTitle')} detail={t('equipment.notFoundDetail')} />
  }

  const memberships = activeMemberships(session.data?.memberships ?? [])
  const villageId = ownVillageId(memberships)
  const personId = session.data?.appUser?.person_id ?? undefined

  if (submit.isSuccess) {
    return (
      <section className="flex max-w-lg flex-col gap-2.5" data-testid="request-success">
        <h1 className="type-screen-title">{t('equipment.successTitle')}</h1>
        <p style={{ fontSize: 15, lineHeight: 1.55, color: 'var(--ink-2)' }}>
          {t('equipment.successDetail')}
        </p>
        {submit.data?.id && (
          <Link
            to="/farm/requests/$requestId"
            params={{ requestId: submit.data.id }}
            data-testid="request-view"
            className="inline-flex w-full items-center justify-center font-semibold"
            style={{
              minHeight: 48,
              border: '1.5px solid var(--primary)',
              borderRadius: 'var(--radius-control)',
              background: 'var(--primary-tint)',
              color: 'var(--primary-ink)',
              fontSize: 16,
            }}
          >
            {t('equipment.viewRequest')}
          </Link>
        )}
      </section>
    )
  }

  const canRequest = Boolean(villageId && personId)

  /**
   * QA #23. `isSubmitting` is react-hook-form's own in-flight flag and it is
   * set synchronously when the handler starts, which `submit.isPending` is
   * not — validation is asynchronous, so the mutation has not begun on the
   * tick a second submit arrives. The button below is disabled on both.
   */
  const onSubmit = handleSubmit((form) => {
    if (!draft.ready) return
    // A choice to make, not a rule to copy: when both are offered the farmer
    // has to say which. The database still decides what is offered.
    if (!isAcquisition(form.acquisition)) {
      setAcquisitionMissing(true)
      return
    }
    void finishDraftWhenSaved(submit.mutateAsync({
      acquisition: form.acquisition,
      id: draft.clientRef,
      actorId: session.data!.userId,
      villageId: villageId!,
      personId: personId!,
      equipmentId: item.id,
      quantity: Number(form.quantity),
      hoursPerDay: Number(form.hours_per_day),
      daysPerWeek: Number(form.days_per_week),
      purpose: form.purpose,
    }), draft.finish)
  })

  /** One message per reason. The schema's `message` holds an i18n key. */
  const err = (name: keyof RequestForm) => {
    const error = errors[name]
    if (!error) return null
    return (
      <p
        id={`request-${fieldId(name)}-error`}
        data-testid={`request-${fieldId(name)}-error`}
        className="flex items-start gap-[7px] font-medium"
        style={{ fontSize: 13, color: 'var(--flag-ink)', textWrap: 'pretty' }}
      >
        <BangMark />
        {t(error.message ?? 'equipment.required')}
      </p>
    )
  }

  return (
    <section className="flex max-w-lg flex-col gap-4" data-testid="equipment-detail">
      <header className="flex flex-col gap-2">
        <p className="type-section" style={{ color: 'var(--ink-3)' }}>
          {item.category_name}
        </p>
        <h1 className="type-screen-title">{item.name}</h1>
        <div data-testid="equipment-specs" className="flex flex-wrap gap-2.5">
          <span className="flex flex-col gap-0.5 px-3 py-2" style={SPEC_CELL}>
            <span className="type-note" style={{ color: 'var(--ink-2)' }}>
              {t('equipment.ratedPower')}
            </span>
            <span className="tabular font-semibold" style={{ fontSize: 17 }}>
              {formatKw(item.rated_power_kw)}
            </span>
          </span>
          {item.can_buy && (
            <span data-testid="equipment-price" className="flex flex-col gap-0.5 px-3 py-2" style={SPEC_CELL}>
              <span className="type-note inline-flex flex-wrap items-center gap-2" style={{ color: 'var(--ink-2)' }}>
                {t('equipment.price')}
                <IndicativePill />
              </span>
              <span className="tabular font-semibold" style={{ fontSize: 17 }}>
                {formatMoney(item.indicative_price, item.currency)}
              </span>
            </span>
          )}
          {item.can_rent && (
            <span data-testid="equipment-rent" className="flex flex-col gap-0.5 px-3 py-2" style={SPEC_CELL}>
              <span className="type-note inline-flex flex-wrap items-center gap-2" style={{ color: 'var(--ink-2)' }}>
                {t('resources.rentPerDay')}
                <IndicativePill />
              </span>
              <span className="tabular font-semibold" style={{ fontSize: 17 }}>
                {formatMoney(item.indicative_rent_per_day, item.currency)}
              </span>
            </span>
          )}
        </div>
        <p style={{ fontSize: 13, color: 'var(--ink-2)', textWrap: 'pretty' }}>
          {t('equipment.notAQuotation')}
        </p>
      </header>

      <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
        {t('equipment.requestThis')}
      </h2>

      <form className="flex flex-col gap-4" noValidate onSubmit={onSubmit}>
        <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
        {/* Everything the person is invited to change, and the estimate that answers it —
            one anchor for the guided tour, which cuts a spotlight around it. The submit
            button is deliberately outside. */}
        <div className="flex flex-col gap-4">
          <fieldset data-testid="equipment-acquisition" className="flex flex-col gap-2">
            <legend style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
              {t('equipment.acquisition')}
            </legend>
            {only ? (
              <p style={{ fontSize: 15, fontWeight: 600 }}>{t(`equipment.acquisitionOnly.${only}`)}</p>
            ) : (
              <div className="flex flex-wrap gap-2">
                {offered.map((mode) => (
                  <label
                    key={mode}
                    className="flex items-center justify-center gap-2 p-2.5"
                    style={{
                      flex: '1 1 120px',
                      minHeight: 48,
                      border: `1.5px solid ${acquisition === mode ? 'var(--primary)' : 'var(--rule-2)'}`,
                      borderRadius: 'var(--radius-control)',
                      background: acquisition === mode ? 'var(--primary-tint)' : 'var(--paper)',
                      fontSize: 15,
                      fontWeight: acquisition === mode ? 600 : 400,
                    }}
                  >
                    <input
                      type="radio"
                      name="acquisition"
                      data-testid={`equipment-acquisition-${mode}`}
                      value={mode}
                      checked={acquisition === mode}
                      onChange={() => {
                        setAcquisition(mode)
                        setAcquisitionMissing(false)
                      }}
                      style={{ accentColor: '#1d70b7' }}
                    />
                    {t(`resources.offered.${mode}`)}
                  </label>
                ))}
              </div>
            )}
            {acquisitionMissing && !isAcquisition(acquisition) && (
              <p
                data-testid="request-acquisition-error"
                className="flex items-start gap-[7px] font-medium"
                style={{ fontSize: 13, color: 'var(--flag-ink)' }}
              >
                <BangMark />
                {t('equipment.chooseAcquisition')}
              </p>
            )}
          </fieldset>

          <div data-testid="request-inputs" className="flex flex-col gap-3">
            <NumberField label={t('equipment.quantity')} testId="request-quantity">
              <input
                id="request-quantity"
                data-testid="request-quantity"
                inputMode="numeric"
                className={inputClass}
                style={errors.quantity ? { ...inputStyle, border: '1.5px solid var(--flag-ink)' } : inputStyle}
                {...(errors.quantity
                  ? { 'aria-invalid': true as const, 'aria-describedby': 'request-quantity-error' }
                  : {})}
                value={quantity}
                onChange={(event) => setQuantity(event.target.value)}
              />
            </NumberField>
            {err('quantity')}

            <NumberField label={t('equipment.hours')} testId="request-hours">
              <input
                id="request-hours"
                data-testid="request-hours"
                inputMode="decimal"
                className={inputClass}
                style={errors.hours_per_day ? { ...inputStyle, border: '1.5px solid var(--flag-ink)' } : inputStyle}
                {...(errors.hours_per_day
                  ? { 'aria-invalid': true as const, 'aria-describedby': 'request-hours-error' }
                  : {})}
                value={hoursPerDay}
                onChange={(event) => setHoursPerDay(event.target.value)}
              />
            </NumberField>
            {err('hours_per_day')}

            <NumberField label={t('equipment.days')} testId="request-days">
              <input
                id="request-days"
                data-testid="request-days"
                inputMode="decimal"
                className={inputClass}
                style={errors.days_per_week ? { ...inputStyle, border: '1.5px solid var(--flag-ink)' } : inputStyle}
                {...(errors.days_per_week
                  ? { 'aria-invalid': true as const, 'aria-describedby': 'request-days-error' }
                  : {})}
                value={daysPerWeek}
                onChange={(event) => setDaysPerWeek(event.target.value)}
              />
            </NumberField>
            {err('days_per_week')}

            <div className="flex flex-col gap-1.5">
              <label
                className="block"
                htmlFor="request-purpose"
                style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}
              >
                {t('equipment.purpose')}
              </label>
              <textarea
                id="request-purpose"
                data-testid="request-purpose"
                rows={2}
                className={inputClass}
                style={{ ...inputStyle, fontVariantNumeric: 'normal' }}
                value={purpose}
                onChange={(event) => setPurpose(event.target.value)}
              />
            </div>
          </div>

          {/* Recalculates live as the assumptions change — but only from
              assumptions that could be true. A figure computed from 99 hours a
              day is not an estimate, it is a wrong answer stated confidently. */}
          {parsed.success ? (
            <EnergyEstimatePanel
              ratedPowerKw={item.rated_power_kw ?? 0}
              quantity={Number(parsed.data.quantity)}
              hoursPerDay={Number(parsed.data.hours_per_day)}
              daysPerWeek={Number(parsed.data.days_per_week)}
            />
          ) : (
            <div data-testid="estimate-blocked">
              <EmptyState
                title={t(
                  anythingBlank
                    ? 'equipment.estimateBlockedTitle'
                    : 'equipment.estimateImpossibleTitle',
                )}
                detail={t(
                  anythingBlank
                    ? 'equipment.estimateBlockedDetail'
                    : 'equipment.estimateImpossibleDetail',
                )}
              />
            </div>
          )}
        </div>

        {submit.isError && (
          <div data-testid="request-error">
            <ErrorState error={submit.error} onRetry={() => submit.reset()} />
          </div>
        )}

        <button
          type="submit"
          data-testid="request-submit"
          disabled={!draft.ready || submit.isPending || isSubmitting || !canRequest}
          className="w-full font-semibold disabled:opacity-60"
          style={{
            minHeight: 52,
            border: 0,
            borderRadius: 10,
            background: 'var(--primary)',
            color: '#fff',
            fontSize: 17,
            fontFamily: 'inherit',
            textWrap: 'balance',
          }}
        >
          {submit.isPending || isSubmitting
            ? t('equipment.submitting')
            : t('equipment.submit')}
        </button>
      </form>
    </section>
  )
}

const inputClass = 'w-full'

/** 48px, 17px, tabular — the same control the register form uses. */
const inputStyle = {
  minHeight: 48,
  width: '100%',
  boxSizing: 'border-box' as const,
  border: '1.5px solid var(--rule-2)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--paper)',
  padding: '12px 14px',
  fontSize: 17,
  fontFamily: 'inherit',
  color: 'var(--ink)',
  fontVariantNumeric: 'tabular-nums' as const,
}

const SPEC_CELL = {
  flex: '1 1 140px',
  minWidth: 0,
  border: '1px solid var(--rule)',
  borderRadius: 'var(--radius-control)',
  background: 'var(--sand-2)',
}

function fieldId(name: keyof RequestForm) {
  // `hours_per_day` is labelled `request-hours` on screen, as the spec names
  // it; the schema keys match the COLUMNS, so the two are mapped rather than
  // derived.
  const MAP: Record<keyof RequestForm, string> = {
    quantity: 'quantity',
    hours_per_day: 'hours',
    days_per_week: 'days',
    purpose: 'purpose',
  }
  return MAP[name]
}

function NumberField({
  label,
  testId,
  children,
}: {
  label: string
  testId: string
  children: React.ReactNode
}) {
  return (
    <div className="flex flex-col gap-1.5">
      <label
        className="block"
        htmlFor={testId}
        style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}
      >
        {label}
      </label>
      {children}
    </div>
  )
}
