import { useMemo, useRef, useState } from 'react'
import { z } from 'zod'
import { zodResolver } from '@hookform/resolvers/zod'
import { ChevronDown, ChevronUp } from 'lucide-react'
import { useNavigate } from '@tanstack/react-router'
import { createColumnHelper } from '@tanstack/react-table'
import { useTranslation } from 'react-i18next'

import { DataTable } from '@/components/DataTable'
import { ErrorState } from '@/components/ErrorState'
import { CONTROL } from '@/components/controlStyles'
import { IndicativePill, Loading } from '@/components/controls'
import { FormField as Field } from '@/components/FormField'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { TableSurface } from '@/components/TableSurface'
import { Button } from '@/components/ui/button'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import {
  useCreateDemand,
  useDemandFormOptions,
  useDemands,
  type Demand,
} from '@/features/ops/useDemand'
import { formatKg, formatMoney, formatPlainDate } from '@/lib/format'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { hasAtMostDecimals } from '@/lib/decimals'
import { usePersistentForm } from '@/lib/usePersistentForm'
import { useSession } from '@/app/session'

/** Spec 7.6 — the order book, plus a create form. */
export function DemandListScreen() {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const query = useDemands()
  const options = useDemandFormOptions()
  const create = useCreateDemand()
  const session = useSession()
  const projectScope = session.data?.memberships.find(m => m.role === 'ops' || m.role === 'admin')?.project_id ?? 'project'
  const draft = usePersistentForm('demand-create', projectScope, DEMAND_DEFAULTS, zodResolver(demandSchema))

  const [buyerId, setBuyerId] = draft.field('buyerId')
  const [cropId, setCropId] = draft.field('cropId')
  const [quantity, setQuantity] = draft.field('quantity')
  const [windowStart, setWindowStart] = draft.field('windowStart')
  const [windowEnd, setWindowEnd] = draft.field('windowEnd')
  const [deliveryPoint, setDeliveryPoint] = draft.field('deliveryPoint')
  const [pricePerKg, setPricePerKg] = draft.field('pricePerKg')
  const [qualityNote, setQualityNote] = draft.field('qualityNote')
  const [touched, setTouched] = useState(false)
  const [createOpen, setCreateOpen] = useState(false)
  /**
   * QA #23. `isPending` only becomes true on the NEXT render, so Enter and a
   * click arriving in one tick both passed the disabled check. A ref latches
   * synchronously and is released when the write settles.
   */
  const inFlight = useRef(false)

  const columns = useMemo(() => {
    const col = createColumnHelper<Demand>()
    return [
      col.accessor('buyer_name', { header: t('demand.colBuyer') }),
      col.accessor('crop_name', { header: t('demand.colCrop') }),
      col.accessor('quantity_kg', {
        header: t('demand.colQuantity'),
        meta: { numeric: true },
        cell: (c) => formatKg(c.getValue()),
      }),
      col.accessor((r) => `${r.window_start}|${r.window_end}`, {
        id: 'window',
        header: t('demand.colWindow'),
        cell: (c) => {
          const [start, end] = String(c.getValue()).split('|')
          return `${formatPlainDate(start)} – ${formatPlainDate(end)}`
        },
      }),
      col.accessor('indicative_price_per_kg', {
        header: t('demand.colPrice'),
        meta: { numeric: true },
        // Prices are indicative, never quotations — and the tag travels with
        // the number rather than trailing it in a parenthesis.
        cell: (c) => (
          <span className="inline-flex flex-wrap items-baseline justify-end gap-2">
            <span className="tabular font-semibold">
              {formatMoney(c.getValue(), c.row.original.currency)}
            </span>
            <IndicativePill />
          </span>
        ),
      }),
      col.accessor('quality_note', {
        header: t('demand.qualityNote'),
        cell: (c) => c.getValue() ?? '—',
      }),
      col.accessor('status', {
        header: t('demand.colStatus'),
        cell: (c) => <StatusPill kind="demand" status={c.getValue()} />,
      }),
    ]
  }, [t])

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  const projectId = options.data?.buyers.find((b) => b.id === buyerId)?.project_id
  const quantityError = quantityErrorKey(quantity)
  const priceError = priceErrorKey(pricePerKg)
  const missing = {
    buyer: touched && buyerId === '',
    crop: touched && cropId === '',
  }

  const submit = () => {
    if (inFlight.current || create.isPending || !draft.ready) return
    setTouched(true)
    if (buyerId === '' || cropId === '' || quantityError || priceError) return
    if (!projectId) return

    inFlight.current = true
    void finishDraftWhenSaved(create.mutateAsync(
      {
        id: draft.clientRef,
        projectId,
        buyerId,
        cropId,
        quantityKg: Number(quantity),
        windowStart,
        windowEnd,
        deliveryPoint,
        pricePerKg,
        qualityNote,
      },
    ), draft.finish).finally(() => { inFlight.current = false })
  }

  return (
    <section className="flex flex-col gap-5">
      <PageHeader
        title={t('demand.title')}
        actions={
          <Button
            type="button"
            data-testid="demand-create-open"
            aria-expanded={createOpen}
            aria-controls="demand-create-panel"
            variant={createOpen ? 'secondary' : 'primary'}
            onClick={() => setCreateOpen((open) => !open)}
          >
            {createOpen ? t('common.close') : t('demand.create')}
            {createOpen ? <ChevronUp aria-hidden size={16} /> : <ChevronDown aria-hidden size={16} />}
          </Button>
        }
      />

      {createOpen && (
        <section
          id="demand-create-panel"
          data-testid="demand-create-panel"
          className="flex w-full flex-col gap-3 p-4 sm:p-[18px]"
          style={{
            border: '1px solid var(--rule)',
            borderRadius: 'var(--radius-card)',
            background: 'var(--paper)',
          }}
        >
          <div className="flex flex-wrap items-center justify-between gap-2">
            <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
              {t('demand.createTitle')}
            </h2>
            <Button
              type="button"
              data-testid="demand-create-close"
              variant="ghost"
              size="sm"
              onClick={() => setCreateOpen(false)}
            >
              {t('common.close')}
            </Button>
          </div>

          {/* A real form: eight fields typed then submitted, so Enter has to
              work and a keyboard user must not have to tab past all of them to
              reach the control. QA #11. */}
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
          <Field label={t('demand.buyer')} id="demand-buyer" error={missing.buyer ? t('demand.required') : undefined} errorTestId="demand-buyer-error">
            <Select value={buyerId} onValueChange={(value) => setBuyerId(value ?? '')}>
              <SelectTrigger id="demand-buyer" data-testid="demand-buyer" className={input}>
                {options.data?.buyers.find((buyer) => buyer.id === buyerId)?.name ?? t('demand.chooseBuyer')}
              </SelectTrigger>
              <SelectContent>
              {(options.data?.buyers ?? []).map((b) => (
                <SelectItem key={b.id} value={b.id}>{b.name}</SelectItem>
              ))}
              </SelectContent>
            </Select>
          </Field>

          <Field label={t('demand.crop')} id="demand-crop" error={missing.crop ? t('demand.required') : undefined} errorTestId="demand-crop-error">
            <Select value={cropId} onValueChange={(value) => setCropId(value ?? '')}>
              <SelectTrigger id="demand-crop" data-testid="demand-crop" className={input}>
                {options.data?.crops.find((crop) => crop.id === cropId)?.name ?? t('demand.chooseCrop')}
              </SelectTrigger>
              <SelectContent>
              {(options.data?.crops ?? []).map((c) => (
                <SelectItem key={c.id} value={c.id}>{c.name}</SelectItem>
              ))}
              </SelectContent>
            </Select>
          </Field>

          <Field label={t('demand.quantity')} id="demand-quantity" error={touched && quantityError ? t(quantityError) : undefined} errorTestId="demand-quantity-error">
            <input id="demand-quantity" data-testid="demand-quantity" inputMode="decimal" value={quantity} onChange={(e) => setQuantity(e.target.value)} className={input} style={CONTROL} />
          </Field>

          <Field label={t('demand.pricePerKg')} id="demand-price" error={touched && priceError ? t(priceError) : undefined} errorTestId="demand-price-error">
            <input id="demand-price" data-testid="demand-price" inputMode="decimal" value={pricePerKg} onChange={(e) => setPricePerKg(e.target.value)} className={input} style={CONTROL} />
          </Field>

          <Field label={t('demand.windowStart')} id="demand-window-start">
            <input id="demand-window-start" data-testid="demand-window-start" type="date" value={windowStart} onChange={(e) => setWindowStart(e.target.value)} className={input} style={CONTROL} />
          </Field>

          <Field label={t('demand.windowEnd')} id="demand-window-end">
            <input id="demand-window-end" data-testid="demand-window-end" type="date" value={windowEnd} onChange={(e) => setWindowEnd(e.target.value)} className={input} style={CONTROL} />
          </Field>

          <Field label={t('demand.deliveryPoint')} id="demand-delivery">
            <input id="demand-delivery" data-testid="demand-delivery" value={deliveryPoint} onChange={(e) => setDeliveryPoint(e.target.value)} className={input} style={CONTROL} />
          </Field>

          <Field label={t('demand.qualityNote')} id="demand-quality-note">
            <input id="demand-quality-note" data-testid="demand-quality-note" value={qualityNote} onChange={(e) => setQualityNote(e.target.value)} className={input} style={CONTROL} />
          </Field>
        </div>

        {create.isError && (
          <div data-testid="demand-create-error">
            <ErrorState error={create.error} onRetry={() => create.reset()} />
          </div>
        )}

            <Button
              type="submit"
              data-testid="demand-create-submit"
              disabled={!draft.ready || create.isPending}
              className="w-fit"
            >
              {create.isPending ? t('demand.creating') : t('demand.create')}
            </Button>
          </form>
        </section>
      )}

      {query.isLoading ? (
        <Loading testId="demand-loading" />
      ) : (
        <TableSurface>
          <DataTable
            columns={columns}
            data={query.demands}
            testId="demand-table"
            rowTestId="demand-row"
            onRowClick={(row) => void navigate({ to: '/ops/demand/$demandId', params: { demandId: row.id } })}
            empty={{ title: t('demand.noneTitle'), detail: t('demand.noneDetail') }}
          />
        </TableSurface>
      )}
    </section>
  )
}

const demandSchema = z.object({
  buyerId: z.string(), cropId: z.string(), quantity: z.string(), windowStart: z.string(),
  windowEnd: z.string(), deliveryPoint: z.string(), pricePerKg: z.string(), qualityNote: z.string(),
})
const DEMAND_DEFAULTS = {
  buyerId: '', cropId: '', quantity: '', windowStart: '', windowEnd: '',
  deliveryPoint: '', pricePerKg: '', qualityNote: '',
}

const input = 'w-full'

/**
 * `quantity_kg` and `indicative_price_per_kg` are both `numeric(12,2)`, which
 * rounds a third decimal silently instead of refusing it — so the form refuses
 * it. And both travel as text: `Number('12,5')` is NaN, which the create hook
 * sent as a null price, dropping what was typed. A comma is refused with a
 * message, not guessed at. Returns an i18n key, or undefined when the value is
 * fine.
 */
function quantityErrorKey(value: string): string | undefined {
  const text = value.trim()
  if (text === '') return 'demand.required'
  const n = Number(text)
  if (!Number.isFinite(n)) return 'demand.notANumber'
  if (!(n > 0)) return 'demand.required'
  if (!hasAtMostDecimals(n, 2)) return 'demand.twoDecimals'
  return undefined
}

/** The price is optional; when given it has to be a real, sendable figure. */
function priceErrorKey(value: string): string | undefined {
  const text = value.trim()
  if (text === '') return undefined
  const n = Number(text)
  if (!Number.isFinite(n)) return 'demand.priceNotANumber'
  // A negative price is left to the demand's check constraint (CLAUDE.md §5):
  // it is rejected with the database's own message, not a client copy.
  if (!hasAtMostDecimals(n, 2)) return 'demand.twoDecimals'
  return undefined
}
