import type { ReactNode } from 'react'
import { useRef, useState } from 'react'
import { getRouteApi } from '@tanstack/react-router'
import { useTranslation } from 'react-i18next'
import { zodResolver } from '@hookform/resolvers/zod'

import { DrillLink } from '@/components/DrillLink'
import { EmptyState } from '@/components/EmptyState'
import { ErrorState } from '@/components/ErrorState'
import { BUTTON_PRIMARY, BUTTON_SECONDARY, CONTROL } from '@/components/controlStyles'
import { Loading, ProductNote, TableCard } from '@/components/controls'
import { BangMark } from '@/components/marks'
import { PageHeader } from '@/components/PageHeader'
import { StatusPill } from '@/components/StatusPill'
import { FormDraftStatus } from '@/components/FormDraftStatus'
import { Select, SelectContent, SelectItem, SelectTrigger } from '@/components/ui/select'
import { supplySchema } from '@/features/ops/supplySchema'
import {
  OPPORTUNITY_ACTION_TARGET,
  opportunityActions,
  releasesSupply,
  type OpportunityAction,
} from '@/features/ops/opportunityTransitions'
import {
  useAttachSupply,
  useAvailableHarvest,
  useOpportunity,
  useOpportunityStatus,
} from '@/features/ops/useOpportunity'
import { formatKg, formatPlainDate, formatTimestamp } from '@/lib/format'
import { useActorName } from '@/lib/actorNames'
import { finishDraftWhenSaved } from '@/lib/drafts'
import { usePersistentForm } from '@/lib/usePersistentForm'

const route = getRouteApi('/_ops/ops/opportunities/$opportunityId')

/** The four controls the machine can offer, in translation-key form. */
const ACTION_LABEL: Record<OpportunityAction, string> = {
  share: 'opportunity.actionShare',
  accept: 'opportunity.actionAccept',
  decline: 'opportunity.actionDecline',
  lapse: 'opportunity.actionLapse',
}

/**
 * Spec 7.8 — the opportunity and its supply lines.
 *
 * opportunity_supply -> harvest_report -> crop_cycle -> plot -> farm IS the
 * traceability claim: every line drills to the record under it.
 *
 * The over-commitment error from opportunity_supply_guard is shown as written.
 * There is deliberately no client-side pre-check — a copy of the guard drifts,
 * and the message names the real numbers.
 *
 * Status is the screen's other action. Declining or lapsing RELEASES the
 * committed supply (business-rules §7) and cannot be undone, so both go
 * through a confirmation that names the kilograms going back. There is no
 * "detach" control: `opportunity_supply` has no DELETE policy and no
 * `deleted_at`, and the release is how a wrong commitment is unwound.
 */
export function OpportunityDetailScreen() {
  const { opportunityId } = route.useParams()
  const { t } = useTranslation()
  const query = useOpportunity(opportunityId)
  const opportunity = query.opportunity
  const available = useAvailableHarvest(opportunity?.village_id, opportunity?.crop_id)
  const attach = useAttachSupply(opportunityId, opportunity?.village_id)
  const move = useOpportunityStatus(
    opportunityId,
    opportunity?.village_id,
    opportunity?.buyer_demand_id ?? undefined,
  )

  const draft = usePersistentForm('opportunity-supply', opportunityId, SUPPLY_DEFAULTS, zodResolver(supplySchema))
  const [harvestId, setHarvestId] = draft.field('harvest_report_id')
  const [kg, setKg] = draft.field('contributed_kg')
  /** Field errors from `supplySchema`, as i18n keys. */
  const [attachErrors, setAttachErrors] = useState<Record<string, string>>({})
  /**
   * QA #23. `isPending` only becomes true on the NEXT render, so Enter and a
   * click in one tick both passed the disabled check. A ref latches
   * synchronously and is released when the write settles.
   */
  const attachInFlight = useRef(false)
  /** The releasing action awaiting confirmation, if any. */
  const [confirming, setConfirming] = useState<OpportunityAction | null>(null)

  if (query.error) return <ErrorState error={query.error} onRetry={() => void query.refetch()} />

  if (query.isLoading) {
    return (
      <Loading testId="opportunity-loading" />
    )
  }

  // Zero rows is an answer: RLS says this opportunity is not visible.
  if (!opportunity) {
    return <EmptyState title={t('opportunity.notFoundTitle')} detail={t('opportunity.notFoundDetail')} />
  }

  const rows = available.data ?? []
  const selectedHarvest = rows.find((row) => row.harvest_report_id === harvestId)
  const actions = opportunityActions(opportunity.status)
  const released = actions.length === 0
  const offered = formatKg(opportunity.offered_quantity_kg)

  /**
   * Validates the column's own shape, then sends.
   *
   * Over-commitment is NOT checked here: `opportunity_supply_guard` owns it,
   * its message names the actual kilograms, and business-rules §8 says to show
   * it as written. So a figure past `available_kg` is sent on purpose.
   */
  function attachSupply() {
    if (attachInFlight.current || attach.isPending || !draft.ready) return

    const parsed = supplySchema.safeParse({ harvest_report_id: harvestId, contributed_kg: kg })
    if (!parsed.success) {
      setAttachErrors(
        Object.fromEntries(
          parsed.error.issues.map((issue) => [String(issue.path[0]), issue.message]),
        ),
      )
      return
    }

    setAttachErrors({})
    const row = rows.find((r) => r.harvest_report_id === parsed.data.harvest_report_id)
    attachInFlight.current = true
    void finishDraftWhenSaved(attach.mutateAsync(
      {
        id: draft.clientRef,
        harvestReportId: parsed.data.harvest_report_id,
        cropCycleId: row?.crop_cycle_id ?? '',
        contributedKg: Number(parsed.data.contributed_kg),
      },
    ), draft.finish).finally(() => { attachInFlight.current = false })
  }

  const attachError = (field: 'harvest_report_id' | 'contributed_kg', testId: string) =>
    attachErrors[field] ? (
      <p
        data-testid={`${testId}-error`}
        className="flex items-start gap-[7px] font-medium"
        style={{ fontSize: 13, color: 'var(--flag-ink)' }}
      >
        <BangMark />
        {t(attachErrors[field])}
      </p>
    ) : null

  function act(action: OpportunityAction) {
    // Forward moves go straight through; the two that release supply and
    // cannot be reversed ask first.
    if (releasesSupply(action)) setConfirming(action)
    else move.mutate(OPPORTUNITY_ACTION_TARGET[action])
  }

  return (
    <section className="flex w-full flex-col gap-[18px]" data-testid="opportunity-detail">
      <PageHeader
        title={<>{opportunity.buyer_name} · {opportunity.village_name} <StatusPill kind="opportunity" status={opportunity.status} /></>}
        description={opportunity.crop_name}
        backTo={opportunity.buyer_demand_id ? `/ops/demand/${opportunity.buyer_demand_id}` : '/ops/demand'}
        backLabel={t('tour.back')}
        breadcrumbs={[
          { label: t('nav.demand'), to: '/ops/demand' },
          { label: opportunity.buyer_name },
        ]}
      />

        {/*
          Two quantities, and the screen's whole job is keeping them apart —
          "Quantity" named neither of them (QA #12). They sit in two cards, only
          one of them tinted: what the buyer asked for is context, what this
          village has offered is the figure this screen is about.
        */}
        <div data-testid="opportunity-quantities" className="flex flex-wrap gap-2.5">
          <Quantity
            label={t('opportunity.demandQuantity')}
            value={formatKg(opportunity.demand_quantity_kg)}
            rowTestId="demand-quantity-row"
          />
          <Quantity
            label={t('opportunity.offered')}
            value={offered}
            testId="offered-total"
            offered
          />
        </div>
        <p data-testid="offered-total-note" className="type-note" style={{ color: 'var(--ink-3)', textWrap: 'pretty' }}>
          {t('opportunity.offeredNote')}
        </p>
      {/* An opportunity is not a sale. Stated on every opportunity surface. */}
      <ProductNote>{t('opportunity.notASale')}</ProductNote>

      {released ? (
        // Declined and lapsed are terminal, so there is nothing to offer —
        // only an explanation of where the supply went. The lines stay listed
        // below: the record of what was offered is not erased.
        <p
          data-testid="released-note"
          className="px-3.5 py-3"
          style={{
            border: '1px solid var(--rule-2)',
            borderRadius: 'var(--radius-card)',
            background: 'var(--paper)',
            fontSize: 14,
            lineHeight: 1.55,
            color: 'var(--ink-2)',
            textWrap: 'pretty',
          }}
        >
          {t('opportunity.releasedNote', { kg: offered })}
        </p>
      ) : confirming ? (
        <div
          data-testid="release-confirm"
          role="alertdialog"
          aria-label={t('opportunity.releaseTitle')}
          className="flex flex-col gap-2.5 px-4 py-3.5"
          style={{
            border: '1px solid rgba(158, 27, 27, .25)',
            borderLeft: '4px solid var(--flag-ink)',
            borderRadius: 'var(--radius-card)',
            background: 'var(--flag-tint)',
          }}
        >
          <p
            className="inline-flex items-center gap-2"
            style={{ fontSize: 15, fontWeight: 600, color: 'var(--flag-ink)' }}
          >
            <BangMark size={18} />
            {t('opportunity.releaseTitle')}
          </p>
          <p style={{ fontSize: 14, lineHeight: 1.55, textWrap: 'pretty' }}>
            {t('opportunity.releaseDetail', { kg: offered })}
          </p>
          <div className="flex flex-wrap gap-2.5">
            <button
              type="button"
              data-testid="release-confirm-yes"
              onClick={() => {
                move.mutate(OPPORTUNITY_ACTION_TARGET[confirming])
                setConfirming(null)
              }}
              style={{
                ...BUTTON_PRIMARY,
                background: 'var(--flag-ink)',
              }}
            >
              {t('opportunity.releaseYes')}
            </button>
            <button
              type="button"
              data-testid="release-confirm-no"
              onClick={() => setConfirming(null)}
              style={BUTTON_SECONDARY}
            >
              {t('opportunity.releaseNo')}
            </button>
          </div>
        </div>
      ) : (
        <div data-testid="opportunity-actions" className="flex flex-wrap items-center gap-2.5">
          {actions.map((action) => (
            <button
              key={action}
              type="button"
              data-testid={`action-${action}`}
              disabled={move.isPending}
              onClick={() => act(action)}
              className="disabled:opacity-60"
              style={
                releasesSupply(action)
                  ? {
                      ...BUTTON_SECONDARY,
                      borderColor: 'rgba(158, 27, 27, .4)',
                      color: 'var(--flag-ink)',
                      fontWeight: 600,
                    }
                  : BUTTON_PRIMARY
              }
            >
              {t(ACTION_LABEL[action])}
            </button>
          ))}
          {/* One write, so one claim about it — rather than every button
              announcing that it is the one saving. */}
          {move.isPending && (
            <span data-testid="status-saving" style={{ fontSize: 14, color: 'var(--ink-2)' }}>
              {t('opportunity.moving')}
            </span>
          )}
        </div>
      )}

      {/* The database's message, as written. §9's error contract. */}
      {move.isError && (
        <div data-testid="status-error">
          <ErrorState error={move.error} onRetry={() => move.reset()} />
        </div>
      )}

      <section className="flex flex-col gap-2.5">
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('opportunity.supplyLines')}
        </h2>

        {opportunity.supply.length === 0 ? (
          <EmptyState
            title={t('opportunity.noSupplyTitle')}
            detail={t('opportunity.noSupplyDetail')}
          />
        ) : (
          <TableCard>
            <div data-testid="supply-lines" className="overflow-x-auto">
            <table className="w-full border-collapse" style={{ fontSize: 15 }}>
              <thead>
                <tr style={{ background: 'var(--sand-2)' }}>
                  <Th>{t('opportunity.colFarmer')}</Th>
                  <Th>{t('opportunity.colPlot')}</Th>
                  <Th>{t('opportunity.colCycle')}</Th>
                  <Th>{t('opportunity.colAttachedBy')}</Th>
                  <Th numeric>{t('opportunity.colContributed')}</Th>
                </tr>
              </thead>
              <tbody>
                {opportunity.supply.map((line) => (
                  <tr
                    key={line.harvest_report_id}
                    data-testid="supply-row"
                    style={{ borderTop: '1px solid var(--rule)' }}
                  >
                    <td className="px-3.5 py-3">
                      <DrillLink kind="person" id={line.person_id}>
                        {line.farmer ?? '—'}
                      </DrillLink>
                    </td>
                    <td className="px-3.5 py-3" style={{ color: 'var(--ink-2)' }}>
                      {line.plot_label ?? '—'}
                    </td>
                    <td className="px-3.5 py-3">
                      <DrillLink kind="cycle" id={line.crop_cycle_id}>
                        {line.crop_name}
                      </DrillLink>
                    </td>
                    <td className="px-3.5 py-3">
                      <AttachedBy actorId={line.captured_by} at={line.captured_at} />
                    </td>
                    <td className="tabular px-3.5 py-3 text-right font-semibold">
                      {formatKg(line.contributed_kg)}
                    </td>
                  </tr>
                ))}
              </tbody>
              {/* The offered total restated at the foot of the lines that make
                  it up, so the header figure and these rows visibly agree. */}
              <tfoot>
                <tr style={{ borderTop: '1px solid var(--rule-2)', background: 'var(--sand-2)' }}>
                  <td colSpan={4} className="px-3.5 py-3" style={{ fontSize: 13, color: 'var(--ink-2)' }}>
                    {t('opportunity.offered')}
                  </td>
                  <td className="tabular px-3.5 py-3 text-right font-semibold" style={{ fontSize: 17 }}>
                    {offered}
                  </td>
                </tr>
              </tfoot>
            </table>
            </div>
          </TableCard>
        )}
      </section>

      <section
        data-testid="opportunity-attach"
        className="flex w-full flex-col gap-3 p-4 sm:p-[18px]"
        style={{
          border: '1px solid var(--rule)',
          borderRadius: 'var(--radius-card)',
          background: 'var(--paper)',
        }}
      >
        <h2 className="type-section" style={{ color: 'var(--ink-3)' }}>
          {t('opportunity.attachTitle')}
        </h2>

        {released ? (
          // committed_kg only sums live opportunities, so a line attached
          // here would be a commitment against nothing.
          <div data-testid="attach-closed">
            <EmptyState
              title={t('opportunity.attachClosedTitle')}
              detail={t('opportunity.attachClosedDetail')}
            />
          </div>
        ) : available.isLoading ? (
          <Loading />
        ) : rows.length === 0 ? (
          <EmptyState
            title={t('opportunity.noneAvailableTitle')}
            detail={t('opportunity.noneAvailableDetail')}
          />
        ) : (
          // A field-then-submit form, so Enter has to work — QA #11.
          <form
            className="flex flex-col gap-3"
            noValidate
            onSubmit={(e) => {
              e.preventDefault()
              attachSupply()
            }}
          >
            <FormDraftStatus dirty={draft.dirty} storageError={draft.storageError} />
            <div className="flex flex-col gap-1.5">
              <label className="block" htmlFor="attach-harvest" style={LABEL}>
                {t('opportunity.attachHarvest')}
              </label>
              <Select value={harvestId} onValueChange={(value) => setHarvestId(value ?? '')}>
                <SelectTrigger id="attach-harvest" data-testid="attach-harvest" className="w-full">
                  {selectedHarvest ? (
                    <span className="flex min-w-0 flex-col text-left leading-snug">
                      <span data-testid="harvest-chosen-farmer" className="truncate font-semibold">
                        {selectedHarvest.farmer ?? '—'} · {selectedHarvest.plot_label ?? '—'}
                      </span>
                      <span className="truncate text-[13px] text-ink-2">
                        {t('opportunity.harvestOption', {
                          expected: formatKg(selectedHarvest.quantity_kg),
                          available: formatKg(selectedHarvest.available_kg),
                        })}
                        {selectedHarvest.harvest_start
                          ? ` · ${t('opportunity.harvestFrom', { date: formatPlainDate(selectedHarvest.harvest_start) })}`
                          : ''}
                      </span>
                    </span>
                  ) : (
                    t('opportunity.chooseHarvest')
                  )}
                </SelectTrigger>
                <SelectContent>
                {/* Fully committed figures stay listed. Hiding them would be a
                    client-side pre-check of the guard. */}
                {rows.map((r) => (
                  <SelectItem key={r.harvest_report_id} value={r.harvest_report_id ?? ''}>
                    {/* Picking a figure is picking a farmer, so the farmer
                        leads. Every figure carries its own label: an
                        unlabelled number on this screen is exactly the
                        ambiguity the rest of it exists to avoid. QA #12. */}
                    <span className="flex flex-col gap-1 py-0.5">
                      <span data-testid="harvest-option-farmer" className="font-semibold text-ink">
                        {r.farmer ?? '—'}
                      </span>
                      <span className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-0.5 text-[13px] font-normal">
                        <OptionField label={t('opportunity.colPlot')} value={r.plot_label ?? '—'} />
                        <OptionField label={t('opportunity.optionExpected')} value={formatKg(r.quantity_kg)} />
                        <OptionField label={t('opportunity.optionAvailable')} value={formatKg(r.available_kg)} />
                        {r.harvest_start && (
                          <OptionField
                            label={t('opportunity.optionHarvestFrom')}
                            value={formatPlainDate(r.harvest_start)}
                          />
                        )}
                      </span>
                    </span>
                  </SelectItem>
                ))}
                </SelectContent>
              </Select>
              {attachError('harvest_report_id', 'attach-harvest')}
            </div>

            <div className="flex flex-col gap-1.5">
              <label className="block" htmlFor="attach-kg" style={LABEL}>
                {t('opportunity.attachKg')}
              </label>
              <input
                id="attach-kg"
                data-testid="attach-kg"
                inputMode="decimal"
                value={kg}
                onChange={(e) => setKg(e.target.value)}
                className="w-full"
                style={CONTROL}
              />
              {attachError('contributed_kg', 'attach-kg')}
            </div>

            {attach.isError && (
              <div data-testid="attach-error">
                <ErrorState error={attach.error} onRetry={() => attach.reset()} />
              </div>
            )}

            <button
              type="submit"
              data-testid="attach-submit"
              // Enabled while incomplete, deliberately: a dead button gives
              // no reason, and the reason is the point.
              disabled={!draft.ready || attach.isPending}
              className="w-fit disabled:opacity-60"
              style={BUTTON_PRIMARY}
            >
              {attach.isPending ? t('opportunity.attaching') : t('opportunity.attach')}
            </button>
          </form>
        )}
      </section>
    </section>
  )
}

const SUPPLY_DEFAULTS = { harvest_report_id: '', contributed_kg: '' }

/**
 * Who attached a supply line, and when. A seeded line has no actor, and a name
 * the caller may not see comes back as zero rows: both are a dash, never an id.
 */
function AttachedBy({ actorId, at }: { actorId: string | null; at: string | null }) {
  const name = useActorName(actorId)
  return (
    <span data-testid="supply-attached-by" className="flex flex-col leading-snug">
      <span>{name ?? '—'}</span>
      {at && (
        <span className="tabular" style={{ fontSize: 12, color: 'var(--ink-3)' }}>
          {formatTimestamp(at)}
        </span>
      )}
    </span>
  )
}

/** One labelled line of a harvest option; `contents` keeps both cells on the parent grid. */
function OptionField({ label, value }: { label: string; value: string }) {
  return (
    <span data-testid="harvest-option-field" className="contents">
      <span className="text-ink-2">{label} </span>
      <span className="tabular-nums text-ink">{value}</span>
    </span>
  )
}

/**
 * One of the two quantities, in a card of its own. Only the offered total is
 * tinted: it is the figure this screen is about, and what the buyer asked for
 * is the context it sits against.
 */
function Quantity({
  label,
  value,
  testId,
  rowTestId,
  offered = false,
}: {
  label: string
  value: string
  testId?: string
  rowTestId?: string
  offered?: boolean
}) {
  return (
    <div
      data-testid={rowTestId}
      className="flex min-w-0 flex-col gap-1 px-3.5 py-3"
      style={{
        flex: '1 1 180px',
        border: `1px solid ${offered ? 'var(--primary)' : 'var(--rule)'}`,
        borderRadius: 'var(--radius-control)',
        background: offered ? 'var(--primary-tint)' : 'var(--sand-2)',
      }}
    >
      <span className="type-note" style={{ color: offered ? 'var(--primary-ink)' : 'var(--ink-2)' }}>
        {label}
      </span>
      <span
        data-testid={testId}
        className="tabular type-figure"
        style={offered ? { color: 'var(--primary-ink)' } : undefined}
      >
        {value}
      </span>
    </div>
  )
}

function Th({ children, numeric = false }: { children: ReactNode; numeric?: boolean }) {
  return (
    <th
      scope="col"
      className={
        numeric
          ? 'type-column-label px-3.5 py-2.5 text-right'
          : 'type-column-label px-3.5 py-2.5 text-left'
      }
      style={{ color: 'var(--ink-3)' }}
    >
      {children}
    </th>
  )
}

const LABEL = { fontSize: 13, fontWeight: 600, color: 'var(--ink-2)' } as const
