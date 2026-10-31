import { useEffect, useMemo, useRef, useState } from 'react'
import { Link, getRouteApi, useNavigate } from '@tanstack/react-router'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { useFieldArray, useForm, type Resolver } from 'react-hook-form'
import { zodResolver } from '@hookform/resolvers/zod'
import { useTranslation } from 'react-i18next'
import { FileImage, FileText, LocateFixed, Paperclip } from 'lucide-react'

import { activeMemberships, writableVillageIds } from '@/app/membership'
import { useSession } from '@/app/session'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { UnsavedDraftBadge } from '@/components/UnsavedDraftBadge'
import { BangMark, PlannedMark, VerificationMark } from '@/components/marks'
import { Button } from '@/components/ui/button'
import {
  buildRegisterPayload,
  type CycleForm,
  type RegisterForm,
} from '@/features/officer/registerPayload'
import {
  isRegisterDraft,
  registerSchema,
  roundedTo,
  toRegisterForm,
  type CropMeasure,
} from '@/features/officer/registerSchema'
import {
  REGISTER_GROUPS,
  isGroupComplete,
  type RegisterGroup,
} from '@/features/officer/registerProgress'
import { useCrops } from '@/features/officer/useCrops'
import { FarmerLoginCard } from '@/features/officer/FarmerLoginCard'
import { useFarmLocation, type FarmLocationStatus } from '@/features/officer/useFarmLocation'
import { draftKey, indexedDbDraftStore, useDraft } from '@/lib/drafts'
import { newUuid } from '@/lib/ids'
import {
  PLOT_DOCUMENT_ACCEPT,
  isDraftFiles,
  uploadPlotDocuments,
  type DraftFile,
} from '@/lib/plotDocuments'
import { queryKeys, isTowerQueryForVillage } from '@/lib/queryKeys'
import { supabase } from '@/lib/supabase'
import type { Json } from '@/lib/db.types'

/** What app_register_farmer returns. */
interface RegisterResult {
  person_id: string | null
  household_id: string | null
  farm_id: string | null
  plot_id: string | null
  crop_cycle_id: string | null
  harvest_report_id: string | null
  crop_cycle_ids?: string[]
  harvest_report_ids?: string[]
  village_id?: string
  replayed: boolean
}

const EMPTY: RegisterForm = {
  given_name: '',
  family_name: '',
  phone: '',
  household_label: '',
  is_head: true,
  farm_label: '',
  farm_latitude: '',
  farm_longitude: '',
  plot_label: '',
  plot_area_ha: '',
  season_label: '',
  planted_on: '',
  cycles: [],
  confidence: 'medium',
}

/** A crop just ticked: nothing typed for it yet. */
const emptyCycle = (cropId: string): CycleForm => ({
  crop_id: cropId,
  area_ha: '',
  tree_count: '',
  unit_count: '',
  harvest_start: '',
  harvest_end: '',
  harvest_quantity_kg: '',
})

/** Top-level fields with an error line of their own; crops have theirs per card. */
type FlatField = Exclude<keyof RegisterForm, 'cycles'>
type CycleField = Exclude<keyof CycleForm, 'crop_id'>

// By route id rather than by importing the route, which would be circular.
const route = getRouteApi('/_officer/officer/register')

/** One draft id per form instance, so a retry cannot create a second farmer. */
function newClientRef() {
  return newUuid()
}

/**
 * Spec 5.2 — one page, one submit, one RPC.
 *
 * Not five chained client inserts: a phone that loses signal mid-chain leaves
 * orphan rows across five tables. app_register_farmer does the whole thing in
 * one transaction and is idempotent on client_ref, so a retry after a timeout
 * cannot create a second farmer.
 */
export function RegisterScreen() {
  const { t } = useTranslation()
  const session = useSession()
  const cropsQuery = useCrops()
  const queryClient = useQueryClient()

  // The draft id is URL-held: a reload must find the same draft, and it also
  // becomes the RPC's client_ref, so a retry after a timeout replays rather
  // than creating a second farmer.
  const { draft: draftIdFromUrl } = route.useSearch()
  const navigate = useNavigate()

  useEffect(() => {
    if (draftIdFromUrl) return
    void navigate({
      to: '/officer/register',
      search: { draft: newClientRef() },
      replace: true,
    })
  }, [draftIdFromUrl, navigate])

  const clientRef = draftIdFromUrl ?? ''
  /**
   * The shape guard is what stops QA #22: a draft written by an OLDER
   * deployment of this form restored verbatim, rendering `[object Object]` as
   * a farmer's first name and standing ready to submit it.
   */
  const draft = useDraft<RegisterForm>(
    draftKey('register', clientRef),
    indexedDbDraftStore,
    isRegisterDraft,
  )

  /**
   * Title documents picked for the plot. The plot does not exist until the RPC
   * returns, so the files wait in a draft of their own — beside the form's,
   * not inside it, so the form's shape guard is untouched — and are filed
   * against the new plot afterwards. Kept until every one is filed.
   */
  const docsDraft = useDraft<DraftFile[]>(draftKey('register-docs', clientRef), indexedDbDraftStore, isDraftFiles)
  const [files, setFiles] = useState<DraftFile[]>([])
  const [filesRestored, setFilesRestored] = useState(false)
  useEffect(() => {
    if (filesRestored || docsDraft.status === 'restoring') return
    setFiles(docsDraft.draft ?? [])
    setFilesRestored(true)
  }, [docsDraft.status, docsDraft.draft, filesRestored])

  const keepFiles = (next: DraftFile[]) => {
    setFiles(next)
    void (next.length > 0 ? docsDraft.save(next) : docsDraft.clear())
  }
  const pickFiles = (picked: FileList | null) => {
    if (!picked || picked.length === 0) return
    keepFiles([
      ...files,
      ...Array.from(picked).map((file) => ({
        id: newUuid(),
        name: file.name,
        type: file.type,
        size: file.size,
        blob: file,
      })),
    ])
  }

  /** Filing happens after the registration, against the plot it created. */
  const [filedCount, setFiledCount] = useState(0)
  const fileDocs = useMutation({
    mutationFn: (target: { villageId: string; plotId: string; files: DraftFile[] }) =>
      uploadPlotDocuments(target.villageId, target.plotId, target.files),
    onSuccess: (result, target) => {
      setFiledCount((n) => n + result.filed.length)
      keepFiles(target.files.filter((f) => !result.filed.includes(f.id)))
    },
  })
  const fileTarget = useRef<{ villageId: string; plotId: string } | null>(null)
  const fileRemaining = () => {
    if (fileTarget.current && files.length > 0) fileDocs.mutate({ ...fileTarget.current, files })
  }

  // Each crop's mandatory measure field depends on that crop's `measured_by`,
  // which arrives with the crop list, so the schema is built at validation
  // time. A ref holds the current map and the resolver reads it on each
  // submit; a resolver passed directly would freeze the list at first render.
  const measuresRef = useRef<Record<string, CropMeasure>>({})
  const resolver = useMemo<Resolver<RegisterForm, unknown, RegisterForm>>(
    () => (values, context, options) =>
      zodResolver(registerSchema(measuresRef.current))(values, context, options),
    [],
  )

  const { control, register, handleSubmit, watch, reset, setValue, getValues, formState } = useForm<
    RegisterForm,
    unknown,
    RegisterForm
  >({
    defaultValues: EMPTY,
    resolver,
  })

  // Restore once the stored draft arrives. Without this the form would render
  // empty and then repopulate, which reads as data loss.
  const [restored, setRestored] = useState(false)
  useEffect(() => {
    if (restored) return
    if (draft.status === 'restoring') return
    // A draft of the one-crop form restores with its crop as the first.
    if (draft.draft) reset(toRegisterForm(draft.draft))
    setRestored(true)
  }, [draft.status, draft.draft, reset, restored])

  /**
   * Whether an automatic GPS read is allowed to fill and disable the farm
   * coordinate fields. Decided once, from the values restoration left behind:
   * an officer's own typing, or a value already in a restored draft, is never
   * overwritten by a sensor read (the reload-survives-a-draft guarantee).
   * `null` means "not decided yet" — before the draft has finished restoring.
   */
  const [gpsEligible, setGpsEligible] = useState<boolean | null>(null)
  useEffect(() => {
    if (!restored || gpsEligible !== null) return
    const values = getValues()
    setGpsEligible(values.farm_latitude === '' && values.farm_longitude === '')
  }, [restored, gpsEligible, getValues])

  const location = useFarmLocation(gpsEligible === true)

  /**
   * Set when the officer presses "Use my current location" (or Try again): an
   * explicit request, so the read replaces whatever the fields hold. The
   * automatic read on arrival never does — it fills only fields that are still
   * empty when the answer comes back, so typing while it is in flight wins.
   */
  const [wantFill, setWantFill] = useState(false)
  const locate = () => {
    setWantFill(true)
    location.retry()
  }

  useEffect(() => {
    if (location.status !== 'acquired' || !location.coords) return
    if (wantFill) {
      setWantFill(false)
    } else {
      const { farm_latitude, farm_longitude } = getValues()
      if (!gpsEligible || farm_latitude !== '' || farm_longitude !== '') return
    }
    setValue('farm_latitude', location.coords.latitude, { shouldDirty: true })
    setValue('farm_longitude', location.coords.longitude, { shouldDirty: true })
  }, [gpsEligible, wantFill, location.status, location.coords, getValues, setValue])

  // One entry per crop ticked. Keyed by react-hook-form's own field id, so a
  // crop removed from the middle does not shift what was typed for the others.
  const cycleFields = useFieldArray({ control, name: 'cycles' })

  // Autosave on change, via react-hook-form's own subscription rather than an
  // effect over stringified values. Local only — the badge stays up until the
  // RPC returns, because reaching IndexedDB is not a server write.
  const save = draft.save
  useEffect(() => {
    if (!restored) return
    // react-hooks/incompatible-library does not model react-hook-form's
    // subscription API, which returns an unsubscribe handle rather than
    // mutating state. The subscription is torn down below, so the effect is
    // correctly scoped.
    // eslint-disable-next-line react-hooks/incompatible-library
    const subscription = watch((value) => {
      const dirty = Object.entries(value).some(([key, v]) =>
        key === 'cycles'
          ? Array.isArray(v) && v.length > 0
          : v !== EMPTY[key as keyof RegisterForm],
      )
      if (dirty) void save(value as RegisterForm)
    })
    return () => subscription.unsubscribe()
  }, [watch, save, restored])

  const villageIds = writableVillageIds(activeMemberships(session.data?.memberships ?? []))
  const villageId = villageIds[0]

  const measures = useMemo(
    () => Object.fromEntries(cropsQuery.crops.map((c) => [c.id, c.measured_by])) as Record<string, CropMeasure>,
    [cropsQuery.crops],
  )
  measuresRef.current = measures
  const cropName = (id: string) => cropsQuery.crops.find((c) => c.id === id)?.name ?? ''

  // Watched so the form can say what a column's scale will do to what was
  // typed (QA #27), and so each crop card renders the measure its crop uses.
  const plotArea = watch('plot_area_ha')
  const cycles = watch('cycles')
  const confidence = watch('confidence')

  const toggleCrop = (cropId: string) => {
    const index = cycles.findIndex((c) => c.crop_id === cropId)
    if (index >= 0) cycleFields.remove(index)
    else cycleFields.append(emptyCycle(cropId), { shouldFocus: false })
  }

  const submit = useMutation({
    mutationFn: async (form: RegisterForm) => {
      if (!villageId) throw new Error(t('register.noVillage'))

      const payload = buildRegisterPayload(form, { clientRef, villageId, measures })
      // The generated signature types the argument as Json, which is a
      // recursive index-signature type an interface cannot satisfy. The shape
      // itself is asserted by registerPayload.test.ts.
      const { data, error } = await supabase.rpc('app_register_farmer', {
        payload: payload as unknown as Json,
      })
      // Trigger and RPC messages are written to be read by humans; surfaced
      // verbatim rather than replaced with a generic failure.
      if (error) throw new Error(error.message)
      // The RPC returns the ids it created, plus `replayed` when a retry hit
      // the idempotency receipt rather than creating a second farmer.
      return data as unknown as RegisterResult
    },
    onSuccess: async (created) => {
      // The registration is saved; now its title documents, if any.
      if (created?.plot_id && files.length > 0) {
        fileTarget.current = { villageId: created.village_id ?? villageId ?? '', plotId: created.plot_id }
        fileDocs.mutate({ ...fileTarget.current, files })
      }
      // Cleared ONLY after the RPC returned success.
      await draft.clear()
      await queryClient.invalidateQueries({ queryKey: queryKeys.people(villageId ?? '') })
      await queryClient.invalidateQueries({ queryKey: queryKeys.farms(villageId ?? '') })
      await queryClient.invalidateQueries({ predicate: isTowerQueryForVillage(villageId ?? '') })
    },
  })

  if (session.error) return <ErrorState error={session.error} />
  if (cropsQuery.error) return <ErrorState error={cropsQuery.error} />

  if (!draftIdFromUrl || session.isLoading || cropsQuery.isLoading || draft.status === 'restoring') {
    return (
      <p data-testid="register-loading" style={{ fontSize: 15, color: 'var(--ink-2)' }}>
        {t('common.loading')}
      </p>
    )
  }

  // An officer with no village assignment cannot register anyone. Say so
  // rather than letting the RPC refuse after a full form is typed.
  if (!villageId) {
    return <EmptyState title={t('register.noVillage')} detail={t('register.noVillageDetail')} />
  }

  if (submit.isSuccess) {
    const created = submit.data
    return (
      <section className="flex max-w-xl flex-col gap-2.5" data-testid="register-success">
        <h1 className="type-screen-title inline-flex items-center gap-2.5">
          <VerificationMark verification="verified" size={20} />
          {t('register.successTitle')}
        </h1>
        <p style={{ fontSize: 14, lineHeight: 1.55, color: 'var(--ink-2)' }}>
          {t('register.successDetail')}
        </p>
        {/* The farmer's app login, created now and shown once (business-rules §15). */}
        {fileDocs.isPending && (
          <p data-testid="register-documents-uploading" className="type-note" style={{ color: 'var(--ink-2)' }}>
            {t('register.documentsUploading')}
          </p>
        )}
        {!fileDocs.isPending && filedCount > 0 && files.length === 0 && (
          <p data-testid="register-documents-filed" className="type-note inline-flex items-center gap-2" style={{ color: 'var(--green-ink)' }}>
            <VerificationMark verification="verified" size={13} />
            {t('register.documentsFiled', { count: filedCount })}
          </p>
        )}
        {!fileDocs.isPending && (fileDocs.isError || (fileDocs.data && files.length > 0)) && (
          <div data-testid="register-documents-failed" className="flex flex-col gap-2">
            <p className="type-note font-medium" style={{ color: 'var(--flag-ink)' }}>
              {t('register.documentsFailed')}
            </p>
            <ul className="type-note flex flex-col gap-1" style={{ color: 'var(--ink-2)' }}>
              {fileDocs.isError && <li>{(fileDocs.error as Error).message}</li>}
              {(fileDocs.data?.failed ?? []).map((f) => (
                <li key={f.id}>{f.name}: {f.message}</li>
              ))}
            </ul>
            <Button type="button" variant="secondary" data-testid="register-documents-retry" className="w-fit" onClick={fileRemaining}>
              {t('register.documentsRetry')}
            </Button>
          </div>
        )}
        {created?.person_id && <FarmerLoginCard personId={created.person_id} autoIssue />}
        {created?.person_id && (
          <Link
            to="/officer/people/$personId"
            params={{ personId: created.person_id }}
            data-testid="register-view-person"
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
            {t('register.viewPerson')}
          </Link>
        )}
        <button
          type="button"
          data-testid="register-another"
          className="inline-flex w-full items-center justify-center font-medium"
          style={{
            minHeight: 48,
            border: '1.5px solid var(--rule-2)',
            borderRadius: 'var(--radius-control)',
            background: 'var(--paper)',
            color: 'var(--ink)',
            fontSize: 16,
          }}
          onClick={() => {
            reset(EMPTY)
            setRestored(true)
            submit.reset()
            fileDocs.reset()
            fileTarget.current = null
            setFiles([])
            setFiledCount(0)
            // A fresh client_ref, so the next registration is a new one rather
            // than an idempotent replay of the one just completed.
            void navigate({
              to: '/officer/register',
              search: { draft: newClientRef() },
              replace: true,
            })
          }}
        >
          {t('register.registerAnother')}
        </button>
      </section>
    )
  }

  /**
   * One message per reason, not one message per form.
   *
   * The schema puts an i18n KEY in `message`, the way `LoginScreen` does, so
   * this resolves it at render. `register.required` is the fallback for an
   * error react-hook-form raised itself, which carries no message.
   */
  const errorLine = (id: string, message: string | undefined) => (
    <p
      id={`register-${id}-error`}
      data-testid={`register-${id}-error`}
      className="flex items-start gap-[7px] font-medium"
      style={{ fontSize: 13, color: 'var(--flag-ink)', textWrap: 'pretty' }}
    >
      <BangMark />
      {t(message ?? 'register.required')}
    </p>
  )

  const err = (name: FlatField) =>
    formState.errors[name] ? errorLine(fieldId(name), formState.errors[name]?.message) : null

  /** A crop card's own field. Test ids match the one-crop form's. */
  const cycleError = (index: number, name: CycleField) => formState.errors.cycles?.[index]?.[name]
  const cycleErr = (index: number, name: CycleField) => {
    const error = cycleError(index, name)
    return error ? errorLine(CYCLE_FIELD_ID[name], error.message) : null
  }

  // "Choose at least one crop": zod's array issue lands on the list itself.
  const cyclesError = formState.errors.cycles?.message ?? formState.errors.cycles?.root?.message

  /**
   * A field's whole presentation, in one place: the 48px control, and — when it
   * failed validation — the red edge, `aria-invalid` and a pointer at the
   * message, so the failure reaches assistive tech as well as the eye.
   */
  const controlProps = (failed: boolean, errorId: string) => ({
    className: inputClass,
    style: failed ? { ...inputStyle, border: '1.5px solid var(--flag-ink)' } : inputStyle,
    ...(failed ? { 'aria-invalid': true as const, 'aria-describedby': `register-${errorId}-error` } : {}),
  })
  const fieldProps = (name: FlatField) => controlProps(Boolean(formState.errors[name]), fieldId(name))
  const cycleProps = (index: number, name: CycleField) =>
    controlProps(Boolean(cycleError(index, name)), CYCLE_FIELD_ID[name])

  /** The status line for a GPS read that isn't quietly succeeding. */
  const gpsMessageKey = (status: FarmLocationStatus): string | null => {
    switch (status) {
      case 'denied':
        return 'register.gpsDenied'
      case 'unsupported':
        return 'register.gpsUnsupported'
      case 'error':
        return 'register.gpsError'
      default:
        return null
    }
  }

  /** What a column's scale will store, when that is not what was typed. */
  const rounded = (testId: string, value: string | undefined, dp: number) => {
    const stored = roundedTo(value ?? '', dp)
    if (!stored) return null
    return (
      <p
        data-testid={`${testId}-rounded`}
        className="type-note flex items-center gap-[7px]"
        style={{ color: 'var(--ink-2)' }}
      >
        {/* Planned: what is on screen is not yet what will be stored. */}
        <PlannedMark />
        {t('register.roundedNote', { value: stored })}
      </p>
    )
  }

  const values = watch()
  const done = (group: RegisterGroup) => isGroupComplete(group, values, measures)
  const completeCount = REGISTER_GROUPS.filter(done).length

  return (
    <section className="mb-[calc(-1*(var(--tab-bar-height,6rem)+1rem))] flex max-w-xl flex-col gap-[18px]">
      <div
        className="flex flex-col gap-[18px] p-[18px]"
        style={{
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-frame)',
          background: 'var(--paper)',
        }}
      >
        <header className="flex flex-col gap-2">
          <h1 className="type-screen-title">{t('register.title')}</h1>
          <p style={{ fontSize: 15, lineHeight: 1.5, color: 'var(--ink-2)' }}>
            {t('register.intro')}
          </p>
          <div className="flex flex-wrap items-center gap-2.5">
            {/* Provenance is never a user-facing choice on this screen. */}
            <span
              data-testid="register-provenance-note"
              className="type-note inline-flex max-w-full items-center gap-[7px] px-3 py-1.5 font-medium"
              style={{
                border: '1px solid var(--rule-2)',
                borderRadius: 'var(--radius-pill)',
                background: 'var(--primary-tint)',
                color: 'var(--primary-ink)',
              }}
            >
              <VerificationMark verification="verified" size={13} />
              {t('register.provenanceNote')}
            </span>
          </div>
        </header>

        {/*
          A rail, not a wizard page-turner. Six steps say which groups are
          filled in, which is open and what is left — without splitting one
          transaction into six submits. Sized so the last step peeks off the
          edge on a phone: the hint to scroll is the layout, not a scrollbar.
        */}
        <ol
          data-testid="register-progress"
          className="scrollbar-hidden flex items-start overflow-x-auto"
          style={{ paddingBottom: 2 }}
        >
          {REGISTER_GROUPS.map((group, index) => (
            <li key={group} className="flex items-start" style={{ flex: 'none' }}>
              <div className="flex flex-col items-center" style={{ width: 68 }}>
                <span
                  data-group={group}
                  data-complete={done(group) ? 'yes' : 'no'}
                  className="inline-flex items-center justify-center font-semibold"
                  style={{
                    width: 26,
                    height: 26,
                    flex: 'none',
                    borderRadius: 'var(--radius-pill)',
                    border: done(group) ? 'none' : '1.5px solid var(--rule-2)',
                    background: done(group) ? 'var(--green-ink)' : 'var(--paper)',
                    color: done(group) ? '#fff' : 'var(--ink-2)',
                    fontSize: 12,
                  }}
                >
                  {done(group) ? <StepCheck /> : index + 1}
                </span>
                <span
                  className="type-note text-center"
                  style={{
                    marginTop: 4,
                    fontWeight: 600,
                    color: done(group) ? 'var(--green-ink)' : 'var(--ink-2)',
                  }}
                >
                  {t(`register.sections.${group}`)}
                </span>
              </div>
              {index < REGISTER_GROUPS.length - 1 && (
                <span
                  aria-hidden
                  style={{
                    flex: 'none',
                    width: 20,
                    height: 2,
                    marginTop: 12,
                    background: done(group) ? 'var(--green-ink)' : 'var(--rule-2)',
                  }}
                />
              )}
            </li>
          ))}
        </ol>
      </div>

      <form
        className="flex flex-col gap-4"
        noValidate
        // QA #23. `formState.isSubmitting` is set synchronously when the
        // handler starts; `submit.isPending` is not, because validation is
        // asynchronous and the mutation has not begun on the tick the officer
        // clicks again. The control below is disabled on both.
        onSubmit={handleSubmit((form) => submit.mutate(form))}
      >
        <Fieldset number={1} legend={t('register.sections.person')} complete={done('person')} testId="register-group-person">
          <Field label={t('register.givenName')} id="register-given-name">
            <input
              id="register-given-name"
              data-testid="register-given-name"
              {...fieldProps('given_name')}
              {...register('given_name')}
            />
          </Field>
          {err('given_name')}
          <Field label={t('register.familyName')} id="register-family-name">
            <input
              id="register-family-name"
              data-testid="register-family-name"
              {...fieldProps('family_name')}
              {...register('family_name')}
            />
          </Field>
          {err('family_name')}
          <Field label={t('register.phone')} id="register-phone">
            <input
              id="register-phone"
              data-testid="register-phone"
              inputMode="tel"
              {...fieldProps('phone')}
              {...register('phone')}
            />
          </Field>
          {err('phone')}
          {/* The phone is the farmer's login name, so it is required. Whether it
              is a Tanzanian mobile is app_normalize_phone's call, not ours. #28. */}
          <p data-testid="register-phone-hint" className="type-note" style={{ color: 'var(--ink-3)' }}>
            {t('register.phoneHint')}
          </p>
        </Fieldset>

        <Fieldset number={2} legend={t('register.sections.household')} complete={done('household')}>
          <Field label={t('register.householdLabel')} id="register-household-label" optional>
            <input
              id="register-household-label"
              data-testid="register-household-label"
              className={inputClass}
              style={inputStyle}
              {...register('household_label')}
            />
          </Field>
          <label
            className="flex items-center gap-2.5"
            style={{ minHeight: 48, fontSize: 15, color: 'var(--ink)' }}
          >
            <input
              type="checkbox"
              data-testid="register-is-head"
              style={{ width: 20, height: 20, accentColor: '#1d70b7' }}
              {...register('is_head')}
            />
            {t('register.isHead')}
          </label>
        </Fieldset>

        <Fieldset number={3} legend={t('register.sections.farm')} complete={done('farm')} testId="register-group-farm">
          <Field label={t('register.farmLabel')} id="register-farm-label">
            <input
              id="register-farm-label"
              data-testid="register-farm-label"
              {...fieldProps('farm_label')}
              {...register('farm_label')}
            />
          </Field>
          {err('farm_label')}
          <div className="flex flex-wrap gap-3">
            <div style={{ flex: '1 1 150px', minWidth: 0 }}>
            <Field label={t('register.latitude')} id="register-farm-latitude">
              <input
                id="register-farm-latitude"
                data-testid="register-farm-latitude"
                {...fieldProps('farm_latitude')}
              {...register('farm_latitude')}
              />
              {err('farm_latitude')}
            </Field>
            </div>
            <div style={{ flex: '1 1 150px', minWidth: 0 }}>
            <Field label={t('register.longitude')} id="register-farm-longitude">
              <input
                id="register-farm-longitude"
                data-testid="register-farm-longitude"
                {...fieldProps('farm_longitude')}
              {...register('farm_longitude')}
              />
              {err('farm_longitude')}
            </Field>
            </div>
          </div>
          {location.status === 'acquiring' && (
            <p
              data-testid="register-gps-status"
              className="type-note"
              style={{ color: 'var(--ink-3)' }}
            >
              {t('register.gpsDetecting')}
            </p>
          )}
          {gpsMessageKey(location.status) ? (
            <div className="flex flex-wrap items-center gap-2.5">
              <p
                data-testid="register-gps-status"
                className="type-note"
                style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}
              >
                {t(gpsMessageKey(location.status) as string)}
              </p>
              {location.status !== 'unsupported' && (
                <Button
                  type="button"
                  data-testid="register-gps-retry"
                  variant="secondary"
                  size="sm"
                  onClick={locate}
                >
                  {t('register.gpsRetry')}
                </Button>
              )}
            </div>
          ) : (
            <Button
              type="button"
              data-testid="register-gps-locate"
              variant="secondary"
              className="w-full"
              disabled={location.status === 'acquiring'}
              onClick={locate}
            >
              <LocateFixed size={18} aria-hidden="true" />
              {t('register.gpsUseCurrent')}
            </Button>
          )}
          <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
            {t('register.gpsNote')}
          </p>
        </Fieldset>

        <Fieldset number={4} legend={t('register.sections.plot')} complete={done('plot')}>
          <Field label={t('register.plotLabel')} id="register-plot-label">
            <input
              id="register-plot-label"
              data-testid="register-plot-label"
              {...fieldProps('plot_label')}
              {...register('plot_label')}
            />
          </Field>
          {err('plot_label')}
          <Field label={t('register.plotArea')} id="register-plot-area">
            <input
              id="register-plot-area"
              data-testid="register-plot-area"
              inputMode="decimal"
              {...fieldProps('plot_area_ha')}
              {...register('plot_area_ha')}
            />
          </Field>
          {err('plot_area_ha')}
          {rounded('register-plot-area', plotArea, 4)}
          {/* cycle area is "planted area across cycles", never "land area". */}
          <p data-testid="register-plot-area-note" className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
            {t('register.plantedAcrossNote')}
          </p>

          {/* Title documents: what the verifier checks this plot against. */}
          <div className="flex flex-col gap-2">
            <span style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
              {t('register.documents')}
              <span style={{ fontWeight: 400, color: 'var(--ink-3)' }}> · {t('common.optional')}</span>
            </span>
            <p data-testid="register-documents-note" className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
              {t('register.documentsHint')}
            </p>
            {files.length > 0 && (
              <ul className="flex flex-col gap-1.5">
                {files.map((f) => (
                  <li
                    key={f.id}
                    data-testid="register-document-item"
                    className="flex items-center gap-2.5 px-3 py-2"
                    style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-control)', fontSize: 15 }}
                  >
                    {f.type === 'application/pdf' ? <FileText size={18} aria-hidden /> : <FileImage size={18} aria-hidden />}
                    <span className="min-w-0 flex-1 truncate">{f.name}</span>
                    <span className="type-note tabular" style={{ color: 'var(--ink-3)' }}>
                      {formatFileSize(f.size)}
                    </span>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      data-testid="register-document-remove"
                      aria-label={t('register.documentsRemove', { name: f.name })}
                      onClick={() => keepFiles(files.filter((other) => other.id !== f.id))}
                    >
                      {t('register.documentsRemoveShort')}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            {/* A real file input behind a 48px label: the phone offers camera
                or files itself. */}
            <label
              htmlFor="register-plot-documents"
              className="inline-flex w-full cursor-pointer items-center justify-center gap-2 font-medium"
              style={{
                minHeight: 48,
                border: '1.5px dashed var(--rule-2)',
                borderRadius: 'var(--radius-control)',
                background: 'var(--paper)',
                color: 'var(--primary-ink)',
                fontSize: 15,
              }}
            >
              <Paperclip size={18} aria-hidden />
              {t('register.documentsAdd')}
            </label>
            <input
              id="register-plot-documents"
              data-testid="register-plot-documents"
              type="file"
              multiple
              accept={PLOT_DOCUMENT_ACCEPT}
              className="sr-only"
              onChange={(e) => {
                pickFiles(e.target.files)
                e.target.value = ''
              }}
            />
          </div>
        </Fieldset>

        <Fieldset
          number={5}
          legend={t('register.sections.cycle')}
          complete={done('cycle')}
          testId="register-group-cycle"
        >
          <fieldset className="flex flex-col gap-2">
            <legend style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
              {t('register.crops')}
            </legend>
            <p className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
              {t('register.cropsHint')}
            </p>
            <div data-testid="register-crops" className="flex flex-wrap gap-2">
              {cropsQuery.crops.map((c) => (
                <CropTarget
                  key={c.id}
                  id={c.id}
                  label={c.name}
                  selected={cycles.some((cycle) => cycle.crop_id === c.id)}
                  onToggle={() => toggleCrop(c.id)}
                />
              ))}
            </div>
            {cyclesError && errorLine('cycles', cyclesError)}
          </fieldset>

          {cycleFields.fields.map((field, index) => {
            const cropId = cycles[index]?.crop_id ?? field.crop_id
            const measure = measures[cropId]
            const name = cropName(cropId)
            return (
              <div
                key={field.id}
                data-testid="register-cycle-card"
                data-crop-id={cropId}
                className="flex flex-col gap-3.5 p-3.5"
                style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--sand-1, var(--paper))' }}
              >
                <h3 className="type-section" style={{ color: 'var(--ink)' }}>{name}</h3>

                {/* Branches on crop.measured_by: the RPC rejects the wrong
                    measure, so only the right field is offered. */}
                {measure === 'area' && (
                  <Field label={t('register.cycleArea')} id={`register-cycle-area-${index}`} hint={t('register.measuredBy', { crop: name })}>
                    <input
                      id={`register-cycle-area-${index}`}
                      data-testid="register-cycle-area"
                      inputMode="decimal"
                      {...cycleProps(index, 'area_ha')}
                      {...register(`cycles.${index}.area_ha`)}
                    />
                    {cycleErr(index, 'area_ha')}
                    {rounded('register-cycle-area', cycles[index]?.area_ha, 4)}
                  </Field>
                )}
                {measure === 'tree_count' && (
                  <Field label={t('register.treeCount')} id={`register-cycle-tree-count-${index}`} hint={t('register.measuredBy', { crop: name })}>
                    <input
                      id={`register-cycle-tree-count-${index}`}
                      data-testid="register-cycle-tree-count"
                      inputMode="numeric"
                      {...cycleProps(index, 'tree_count')}
                      {...register(`cycles.${index}.tree_count`)}
                    />
                    {cycleErr(index, 'tree_count')}
                  </Field>
                )}
                {measure === 'unit_count' && (
                  <Field label={t('register.unitCount')} id={`register-cycle-unit-count-${index}`} hint={t('register.measuredBy', { crop: name })}>
                    <input
                      id={`register-cycle-unit-count-${index}`}
                      data-testid="register-cycle-unit-count"
                      inputMode="numeric"
                      {...cycleProps(index, 'unit_count')}
                      {...register(`cycles.${index}.unit_count`)}
                    />
                    {cycleErr(index, 'unit_count')}
                  </Field>
                )}

                {/* One column: an iOS date input has an intrinsic minimum width
                    that a half-width column on a phone cannot hold. */}
                <Field label={t('register.harvestStart')} id={`register-harvest-start-${index}`}>
                  <input
                    id={`register-harvest-start-${index}`}
                    data-testid="register-harvest-start"
                    type="date"
                    {...cycleProps(index, 'harvest_start')}
                    {...register(`cycles.${index}.harvest_start`)}
                  />
                  {cycleErr(index, 'harvest_start')}
                </Field>
                <Field label={t('register.harvestEnd')} id={`register-harvest-end-${index}`}>
                  <input
                    id={`register-harvest-end-${index}`}
                    data-testid="register-harvest-end"
                    type="date"
                    {...cycleProps(index, 'harvest_end')}
                    {...register(`cycles.${index}.harvest_end`)}
                  />
                  {cycleErr(index, 'harvest_end')}
                </Field>
              </div>
            )
          })}
        </Fieldset>

        <Fieldset number={6} legend={t('register.sections.harvest')} complete={done('harvest')}>
          {cycleFields.fields.map((field, index) => {
            const name = cropName(cycles[index]?.crop_id ?? field.crop_id)
            return (
              <div key={field.id} data-testid="register-harvest-row" className="flex flex-col gap-1.5">
                <Field label={t('register.harvestKg')} id={`register-harvest-kg-${index}`} hint={name}>
                  <input
                    id={`register-harvest-kg-${index}`}
                    data-testid="register-harvest-kg"
                    inputMode="decimal"
                    {...cycleProps(index, 'harvest_quantity_kg')}
                    {...register(`cycles.${index}.harvest_quantity_kg`)}
                  />
                </Field>
                {cycleErr(index, 'harvest_quantity_kg')}
                {rounded('register-harvest-kg', cycles[index]?.harvest_quantity_kg, 2)}
              </div>
            )
          })}
          {/*
            Three targets rather than a select. Confidence is the one field on
            this form an officer sets from judgement rather than from something
            in front of them, and a 48px target each is quicker to hit outdoors
            than opening a picker.
          */}
          <fieldset className="flex flex-col gap-2">
            <legend style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}>
              {t('register.confidence')}
            </legend>
            <div data-testid="register-confidence" className="flex flex-wrap gap-2">
              {(['low', 'medium', 'high'] as const).map((level) => (
                <ConfidenceTarget
                  key={level}
                  level={level}
                  label={t(`confidence.${level}`)}
                  selected={confidence === level}
                  register={register('confidence')}
                />
              ))}
            </div>
          </fieldset>
        </Fieldset>

        {submit.isError && (
          <div data-testid="register-error">
            <ErrorState error={submit.error} onRetry={() => submit.reset()} />
          </div>
        )}

        {/*
          This stays in normal document flow. A bottom-sticky bar covered the
          final confidence choice on smaller phones, leaving the last card
          visibly cropped. The guided tour scrolls here for step four, while
          ordinary use reaches it after the six field groups; `main` already
          reserves space above the fixed tab bar for flowing content.
        */}
        <div
          data-testid="register-submit-bar"
          // Sits on the tab bar: the register section pulls this bar down over the
          // room `main` reserves for it, and this padding puts that room back as
          // white inside the bar so the button still clears the tab bar.
          className="-mx-4 flex flex-col gap-2.5 px-4 pt-3 pb-[calc(var(--tab-bar-height,6rem)+1rem)]"
          style={{
            background: 'var(--paper)',
            borderTop: '1px solid var(--rule)',
          }}
        >
          <div className="flex flex-wrap items-center justify-between gap-2.5">
            <span className="type-note" style={{ color: 'var(--ink-3)' }}>
              {t('register.groupsComplete', {
                done: completeCount,
                total: REGISTER_GROUPS.length,
              })}
            </span>
            {draft.status === 'dirty' && <UnsavedDraftBadge />}
          </div>
          <button
            type="submit"
            data-testid="register-submit"
            disabled={submit.isPending || formState.isSubmitting}
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
            {submit.isPending || formState.isSubmitting
              ? t('register.submitting')
              : t('register.submit')}
          </button>
        </div>
      </form>
    </section>
  )
}

/**
 * 48px tall, 17px type, and wide enough for Kiswahili.
 *
 * Outdoors, one-handed, with someone waiting: the target has to be reachable
 * with a thumb and the value has to be legible in daylight. `min-height` rather
 * than `height`, because a label that wraps must be allowed to.
 */
const inputClass = 'w-full px-3.5 py-3'

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
  fontVariantNumeric: 'tabular-nums',
}

/** A file's size, for the officer's eye only. */
function formatFileSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

function fieldId(name: FlatField) {
  return name.replace(/_/g, '-')
}

/** The one-crop form's error ids, kept so a crop card's errors read the same. */
const CYCLE_FIELD_ID: Record<CycleField, string> = {
  area_ha: 'cycle-area-ha',
  tree_count: 'cycle-tree-count',
  unit_count: 'cycle-unit-count',
  harvest_start: 'harvest-start',
  harvest_end: 'harvest-end',
  harvest_quantity_kg: 'harvest-quantity-kg',
}

/** One crop to tick, a 48px target like the confidence choices. */
function CropTarget({
  id,
  label,
  selected,
  onToggle,
}: {
  id: string
  label: string
  selected: boolean
  onToggle: () => void
}) {
  return (
    <label
      className="flex items-center gap-2 p-2.5"
      style={{
        flex: '1 1 130px',
        minHeight: 48,
        border: `1.5px solid ${selected ? 'var(--primary)' : 'var(--rule-2)'}`,
        borderRadius: 'var(--radius-control)',
        background: selected ? 'var(--primary-tint)' : 'var(--paper)',
        fontSize: 15,
        fontWeight: selected ? 600 : 400,
        color: selected ? 'var(--primary-ink)' : 'var(--ink-2)',
      }}
    >
      <input
        type="checkbox"
        data-testid={`register-crop-${id}`}
        checked={selected}
        onChange={onToggle}
        style={{ width: 20, height: 20, accentColor: '#1d70b7' }}
      />
      {label}
    </label>
  )
}

/** A numbered section card. The number is the officer's place in the form. */
function Fieldset({
  number,
  legend,
  complete,
  testId,
  children,
}: {
  number: number
  legend: string
  complete: boolean
  /** Lets the guided tour point at a whole group. */
  testId?: string
  children: React.ReactNode
}) {
  return (
    <fieldset
      data-testid={testId}
      data-complete={complete ? 'yes' : 'no'}
      className="flex flex-col gap-3.5 p-[18px]"
      style={{
        border: `1.5px solid ${complete ? 'rgba(63,84,16,0.55)' : 'var(--rule)'}`,
        borderRadius: 'var(--radius-frame)',
        background: 'var(--paper)',
      }}
    >
      <legend className="type-section -ml-1 inline-flex items-center gap-2.5 px-2" style={{ color: 'var(--ink-3)' }}>
        <span
          className="inline-flex items-center justify-center"
          style={{
            width: 20,
            height: 20,
            borderRadius: 'var(--radius-pill)',
            background: complete ? 'var(--green-ink)' : 'var(--sand-2)',
            color: complete ? '#fff' : 'var(--ink-2)',
            fontSize: 12,
            fontWeight: 700,
            letterSpacing: 0,
          }}
        >
          {complete ? <StepCheck /> : number}
        </span>
        {legend}
      </legend>
      {children}
    </fieldset>
  )
}

/**
 * A plain check mark, sized for the 20-26px badges this screen uses to mark a
 * step or a section done. `VerificationMark`'s "verified" glyph draws its own
 * filled circle behind the check — nesting that inside another circle here
 * doubled up the background and looked wrong, so this is the check alone.
 */
function StepCheck() {
  return (
    <span
      aria-hidden
      style={{
        display: 'block',
        width: 7,
        height: 3.5,
        borderLeft: '2px solid currentColor',
        borderBottom: '2px solid currentColor',
        transform: 'rotate(-45deg) translate(0.5px, -1px)',
      }}
    />
  )
}

function Field({
  label,
  id,
  optional = false,
  hint,
  children,
}: {
  label: string
  id: string
  /** Said in the label, not implied by its absence. */
  optional?: boolean
  hint?: string
  children: React.ReactNode
}) {
  const { t } = useTranslation()

  return (
    <div className="flex flex-col gap-1.5">
      <label
        className="flex flex-wrap items-baseline gap-x-2"
        htmlFor={id}
        style={{ fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' }}
      >
        {label}
        {optional && (
          <span style={{ fontWeight: 400, color: 'var(--ink-3)' }}>· {t('common.optional')}</span>
        )}
        {hint && (
          <span style={{ fontWeight: 400, color: 'var(--ink-3)' }}>{hint}</span>
        )}
      </label>
      {children}
    </div>
  )
}

/** One of three confidence targets, carrying the same meter the badge uses. */
function ConfidenceTarget({
  level,
  label,
  selected,
  register,
}: {
  level: 'low' | 'medium' | 'high'
  label: string
  selected: boolean
  register: ReturnType<ReturnType<typeof useForm<RegisterForm>>['register']>
}) {
  const filled = { low: 1, medium: 2, high: 3 }[level]

  return (
    <label
      className="flex items-center justify-center gap-2 p-2.5"
      style={{
        flex: '1 1 90px',
        minHeight: 48,
        border: `1.5px solid ${selected ? 'var(--primary)' : 'var(--rule-2)'}`,
        borderRadius: 'var(--radius-control)',
        background: selected ? 'var(--primary-tint)' : 'var(--paper)',
        fontSize: 15,
        fontWeight: selected ? 600 : 400,
        color: selected ? 'var(--primary-ink)' : 'var(--ink-2)',
      }}
    >
      <input type="radio" value={level} style={{ accentColor: '#1d70b7' }} {...register} />
      <span aria-hidden className="inline-flex items-end gap-[2px]" style={{ height: 11 }}>
        {[5, 8, 11].map((height, index) => (
          <span
            key={height}
            style={{
              width: 3,
              height,
              borderRadius: 1,
              background:
                index < filled
                  ? selected
                    ? 'var(--primary-ink)'
                    : 'var(--ink-2)'
                  : 'var(--rule-2)',
            }}
          />
        ))}
      </span>
      {label}
    </label>
  )
}
