import { useMemo, useRef, useState } from 'react'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { FormField as Field } from '@/components/FormField'
import { PageHeader } from '@/components/PageHeader'
import { TableSurface } from '@/components/TableSurface'
import { CONTROL } from '@/components/controlStyles'
import { Loading } from '@/components/controls'
import { Button } from '@/components/ui/button'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import {
  useCreateVillage,
  useVillageCapacity,
  type VillageCapacity,
} from '@/features/ops/useOpsReference'
import { useSession } from '@/app/session'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { formatKw, formatPlainDate } from '@/lib/format'
import { usePersistentForm } from '@/lib/usePersistentForm'

/** Absent is not zero: a village with no capacity row has no figure, not 0 kW. */
const DASH = '—'

/**
 * Spec 7.9 — villages, with `village_capacity` showing `basis` and
 * `simultaneity_factor` EXPLICITLY.
 *
 * Capacity is planned, never measured. `capacity_basis` has no 'measured'
 * value on purpose, and the basis is rendered beside every figure so a planned
 * number can never be read as a metered one.
 */
export function VillagesScreen() {
  const { t } = useTranslation()
  const query = useVillageCapacity()
  const [createOpen, setCreateOpen] = useState(false)

  const columns = useMemo(() => {
    const col = createColumnHelper<VillageCapacity>()
    return [
      col.accessor('name', { header: t('villages.colName') }),
      col.accessor('code', { header: t('villages.colCode') }),
      /*
        Never presented as measured — and the basis lives INSIDE the capacity
        cell rather than in a column of its own, so no sort can separate a
        planned figure from the word that says it is planned.
      */
      col.accessor('capacity_kw', {
        header: t('villages.colCapacity'),
        meta: { numeric: true },
        cell: (c) => (
          <span className="inline-flex flex-wrap items-baseline justify-end gap-2">
            <span className="tabular font-semibold">
              {c.getValue() === null ? DASH : formatKw(c.getValue() as number)}
            </span>
            <span
              data-testid="village-basis"
              className="type-column-label px-2 py-[3px]"
              style={{
                border: '1px solid var(--rule-2)',
                borderRadius: 'var(--radius-pill)',
                background: 'var(--paper)',
                color: 'var(--ink-2)',
                fontWeight: 700,
              }}
            >
              {c.row.original.basis === null
                ? DASH
                : t(`capacityBasis.${c.row.original.basis}`)}
            </span>
          </span>
        ),
      }),
      col.accessor('simultaneity_factor', {
        header: t('villages.colSimultaneity'),
        meta: { numeric: true },
        cell: (c) => <span className="tabular">{c.getValue() ?? DASH}</span>,
      }),
      col.accessor('effective_from', {
        header: t('villages.colEffectiveFrom'),
        cell: (c) => (c.getValue() ? formatPlainDate(c.getValue() as string) : DASH),
      }),
    ]
  }, [t])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  return (
    <section className="flex flex-col gap-4">
      <PageHeader
        title={t('villages.title')}
        description={<span data-testid="villages-note">{t('villages.plannedNote')}</span>}
        actions={
          <Button
            type="button"
            data-testid="village-create-open"
            aria-expanded={createOpen}
            aria-controls="village-create-panel"
            variant={createOpen ? 'secondary' : 'primary'}
            onClick={() => setCreateOpen((open) => !open)}
          >
            {createOpen ? t('common.close') : t('villages.create')}
            {createOpen ? <ChevronUp aria-hidden size={16} /> : <ChevronDown aria-hidden size={16} />}
          </Button>
        }
      />

      {createOpen && <VillageCreatePanel onClose={() => setCreateOpen(false)} />}

      {query.isLoading ? (
        <Loading testId="villages-loading" />
      ) : (
        <TableSurface>
          <DataTable
            columns={columns}
            data={query.data ?? []}
            testId="villages-table"
            rowTestId="village-row"
            empty={{ title: t('villages.noneTitle'), detail: t('villages.noneDetail') }}
          />
        </TableSurface>
      )}

      <p className="type-note" style={{ color: 'var(--ink-3)' }}>{t('villages.simultaneityNote')}</p>
    </section>
  )
}

const villageSchema = z.object({
  name: z.string(), code: z.string(), latitude: z.string(), longitude: z.string(),
  capacityKw: z.string(), basis: z.string(), simultaneity: z.string(),
  sourceNote: z.string(), effectiveFrom: z.string(),
})
const VILLAGE_DEFAULTS = {
  name: '', code: '', latitude: '', longitude: '',
  capacityKw: '', basis: 'planned', simultaneity: '1.000', sourceNote: '', effectiveFrom: '',
}
const BASES = ['planned', 'nameplate'] as const

/**
 * Only the SHAPE of a number is checked here: `Number('25,5')` is NaN and
 * would reach the RPC as a cast error. Ranges — capacity not negative, the
 * factor in (0, 1], GPS bounds — are the columns' checks, and their messages
 * are shown as the database words them. Returns an i18n key or undefined.
 */
function numberErrorKey(value: string, required: boolean): string | undefined {
  const text = value.trim()
  if (text === '') return required ? 'villages.required' : undefined
  return Number.isFinite(Number(text)) ? undefined : 'villages.notANumber'
}

/**
 * Ops adds a village together with its first PLANNED capacity row — one RPC,
 * because a village with no capacity row has no energy figures and is missing
 * from the Tower. Capacity is planned or nameplate, never measured.
 */
function VillageCreatePanel({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation()
  const create = useCreateVillage()
  const session = useSession()
  const projectId = session.data?.memberships.find((m) => m.role === 'ops' || m.role === 'admin')?.project_id
  const draft = usePersistentForm('village-create', projectId ?? 'project', VILLAGE_DEFAULTS, zodResolver(villageSchema))

  const [name, setName] = draft.field('name')
  const [code, setCode] = draft.field('code')
  const [latitude, setLatitude] = draft.field('latitude')
  const [longitude, setLongitude] = draft.field('longitude')
  const [capacityKw, setCapacityKw] = draft.field('capacityKw')
  const [basis, setBasis] = draft.field('basis')
  const [simultaneity, setSimultaneity] = draft.field('simultaneity')
  const [sourceNote, setSourceNote] = draft.field('sourceNote')
  const [effectiveFrom, setEffectiveFrom] = draft.field('effectiveFrom')
  const [touched, setTouched] = useState(false)
  // Latched synchronously: `isPending` only turns true on the next render.
  const inFlight = useRef(false)

  const errors = {
    name: name.trim() === '' ? 'villages.required' : undefined,
    code: code.trim() === '' ? 'villages.required' : undefined,
    latitude: numberErrorKey(latitude, false),
    longitude: numberErrorKey(longitude, false),
    capacity: numberErrorKey(capacityKw, true),
    simultaneity: numberErrorKey(simultaneity, false),
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
      name: name.trim(),
      code: code.trim(),
      latitude: latitude.trim(),
      longitude: longitude.trim(),
      capacity: {
        capacity_kw: capacityKw.trim(),
        basis: basis as (typeof BASES)[number],
        simultaneity_factor: simultaneity.trim(),
        source_note: sourceNote.trim(),
        effective_from: effectiveFrom,
      },
    }), draft.finish).finally(() => { inFlight.current = false })
  }

  return (
    <section
      id="village-create-panel"
      data-testid="village-create-panel"
      className="flex w-full flex-col gap-3 p-4 sm:p-[18px]"
      style={{ border: '1px solid var(--rule)', borderRadius: 'var(--radius-card)', background: 'var(--paper)' }}
    >
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>{t('villages.createTitle')}</h2>
        <Button type="button" data-testid="village-create-close" variant="ghost" size="sm" onClick={onClose}>
          {t('common.close')}
        </Button>
      </div>

      <form
        className="flex flex-col gap-3"
        noValidate
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
      >
        <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <Field label={t('villages.name')} id="village-name" error={shown('name')} errorTestId="village-name-error">
            <input id="village-name" data-testid="village-name" value={name} onChange={(e) => setName(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
          <Field label={t('villages.code')} id="village-code" error={shown('code')} errorTestId="village-code-error">
            <input id="village-code" data-testid="village-code" value={code} onChange={(e) => setCode(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
          <Field label={t('villages.latitude')} id="village-latitude" hint={t('villages.gpsHint')} error={shown('latitude')} errorTestId="village-latitude-error">
            <input id="village-latitude" data-testid="village-latitude" inputMode="decimal" value={latitude} onChange={(e) => setLatitude(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
          <Field label={t('villages.longitude')} id="village-longitude" error={shown('longitude')} errorTestId="village-longitude-error">
            <input id="village-longitude" data-testid="village-longitude" inputMode="decimal" value={longitude} onChange={(e) => setLongitude(e.target.value)} className="w-full" style={CONTROL} />
          </Field>

          <Field label={t('villages.capacity')} id="village-capacity" error={shown('capacity')} errorTestId="village-capacity-error">
            <input id="village-capacity" data-testid="village-capacity" inputMode="decimal" value={capacityKw} onChange={(e) => setCapacityKw(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
          <Field label={t('villages.basis')} id="village-basis-select">
            <Select value={basis} onValueChange={(value) => setBasis(value ?? 'planned')}>
              <SelectTrigger id="village-basis-select" data-testid="village-basis-select" className="w-full">
                {t(`capacityBasis.${basis}`)}
              </SelectTrigger>
              <SelectContent>
                {BASES.map((b) => <SelectItem key={b} value={b}>{t(`capacityBasis.${b}`)}</SelectItem>)}
              </SelectContent>
            </Select>
          </Field>
          <Field label={t('villages.simultaneity')} id="village-simultaneity" hint={t('villages.simultaneityHint')} error={shown('simultaneity')} errorTestId="village-simultaneity-error">
            <input id="village-simultaneity" data-testid="village-simultaneity" inputMode="decimal" value={simultaneity} onChange={(e) => setSimultaneity(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
          <Field label={t('villages.effectiveFrom')} id="village-effective-from">
            <input id="village-effective-from" data-testid="village-effective-from" type="date" value={effectiveFrom} onChange={(e) => setEffectiveFrom(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
          <Field label={t('villages.sourceNote')} id="village-source-note">
            <input id="village-source-note" data-testid="village-source-note" value={sourceNote} onChange={(e) => setSourceNote(e.target.value)} className="w-full" style={CONTROL} />
          </Field>
        </div>

        <p className="type-note" style={{ color: 'var(--ink-3)' }}>{t('villages.officerNote')}</p>

        {create.isError && (
          <div data-testid="village-create-error">
            <ErrorState error={create.error} onRetry={() => create.reset()} />
          </div>
        )}

        <Button type="submit" data-testid="village-create-submit" disabled={!draft.ready || create.isPending} className="w-fit">
          {create.isPending ? t('villages.creating') : t('villages.create')}
        </Button>
      </form>
    </section>
  )
}
