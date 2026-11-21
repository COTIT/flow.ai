"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  purchaseCostAdjustmentsCreate,
  purchaseCostAdjustmentsPost,
  purchaseCostAdjustmentsPostReview,
  purchaseCostAdjustmentsReverse,
  purchaseCostAdjustmentsReverseReview,
} from "@verzo/api-client/endpoints/purchase-cost-adjustments"
import {
  supplierBillsPost,
  supplierBillsPostReview,
  supplierBillsReverse,
  supplierBillsReverseReview,
  useSupplierBillsMatch,
  useSupplierBillsMatchCandidates,
} from "@verzo/api-client/endpoints/supplier-bills"
import type { CostAdjustmentResource, SupplierBillResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { DialogClose, DialogFooter } from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Skeleton } from "@verzo/ui/components/skeleton"
import { formatDay, formatQty } from "@verzo/ui/lib/format"
import { Field } from "@/components/field"
import { Fact, ReviewDialog, usePayReasons, useReverseInputs } from "@/components/payables"

import { useBookToday } from "../../accounting/shared"
import { showMoney } from "../../purchases/shared"
import { Shell, useAction, type Outcome } from "../../purchases/[id]/dialogs"

type Props = { bill: SupplierBillResource; button: React.ReactElement; outcome: Outcome }

const lineName = (bill: SupplierBillResource, lineId: string) => {
  const l = bill.lines.find((x) => x.id === lineId)
  return l?.product_name ?? l?.description ?? ""
}

/**
 * Match: per goods line, the receipt lines the server says it may still take (with how much),
 * and a quantity for each. Sends the full set; an empty set releases every reservation.
 */
export function MatchDialog({ bill, button, outcome }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = usePayReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const candidates = useSupplierBillsMatchCandidates(workspaceId, bill.id, {
    query: { enabled: a.open && Boolean(workspaceId) },
  })
  const [qty, setQty] = React.useState<Record<string, string>>({})
  const match = useSupplierBillsMatch({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("payables.done.matched"))
      },
    },
  })
  const k = (line: string, rl: string) => `${line}:${rl}`
  const errors = fieldErrors(match.error)
  const goods = bill.lines.filter((l) => l.kind === "goods")
  const rows = candidates.data?.data.candidates ?? []
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        match.reset()
        const now: Record<string, string> = {}
        for (const l of goods)
          for (const m of l.matches) if (m.receipt_line_id) now[k(l.id, m.receipt_line_id)] = m.quantity
        setQty(now)
      }}
      button={button}
      trigger={t("payables.bills.match")}
      title={t("payables.bills.matchTitle", { number: bill.number })}
      description={t("payables.bills.matchText")}
    >
      {candidates.isPending ? (
        <div aria-busy="true" className="flex flex-col gap-2">
          <Skeleton className="h-4 w-2/3" />
          <Skeleton className="h-4 w-1/2" />
        </div>
      ) : candidates.isError ? (
        <ApiError error={candidates.error} fallback={t("common.error")} codes={reasons} />
      ) : (
        <div className="flex max-h-[55vh] flex-col gap-4 overflow-y-auto">
          {goods.map((l) => {
            const mine = rows.filter((c) => c.bill_line_id === l.id)
            return (
              <fieldset key={l.id} className="flex flex-col gap-2">
                <legend className="text-sm font-semibold">
                  {l.product_name} × {formatQty(l.quantity, locale)}
                </legend>
                {mine.length === 0 ? (
                  <p className="text-[13px] text-muted-foreground">{t("payables.bills.noCandidates")}</p>
                ) : (
                  mine.map((c) => (
                    <Field
                      key={c.receipt_line_id}
                      id={`m-${l.id}-${c.receipt_line_id}`}
                      label={t("payables.bills.candidate", {
                        receipt: c.receipt_number,
                        order: c.purchase_order_number,
                        left: formatQty(c.matchable, locale),
                      })}
                    >
                      <Input
                        inputMode="decimal"
                        value={qty[k(l.id, c.receipt_line_id)] ?? ""}
                        onChange={(e) => setQty((q) => ({ ...q, [k(l.id, c.receipt_line_id)]: e.target.value }))}
                      />
                    </Field>
                  ))
                )}
              </fieldset>
            )
          })}
        </div>
      )}
      {Object.entries(errors).length ? (
        <p className="text-sm text-destructive">{Object.values(errors).flat().join(" ")}</p>
      ) : null}
      <ApiError error={match.error} fallback={t("common.error")} codes={reasons} />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>{t("actions.cancel")}</DialogClose>
        <Button
          disabled={match.isPending || candidates.isPending}
          onClick={() =>
            match.mutate({
              workspace: workspaceId,
              bill: bill.id,
              data: {
                client_ref: a.clientRef,
                expected_version: bill.version,
                payload: {
                  allocations: Object.entries(qty)
                    .filter(([, q]) => q.trim() && Number(q) !== 0)
                    .map(([key, q]) => {
                      const [bill_line_id = "", receipt_line_id = ""] = key.split(":")
                      return { bill_line_id, receipt_line_id, quantity: q.trim() }
                    }),
                },
              },
            })
          }
        >
          {t("payables.bills.saveMatch")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/** Post: the server's review (rate, net, tax, total, journal) first, then its fingerprint. */
export function PostBillDialog({ bill, button, outcome }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  return (
    <ReviewDialog
      auto
      button={button}
      outcome={outcome}
      trigger={t("payables.bills.post")}
      title={t("payables.bills.postTitle", { number: bill.number })}
      description={t("payables.bills.postText")}
      done={t("payables.done.posted")}
      review={async () => (await supplierBillsPostReview(ws, bill.id)).data}
      renderReview={(r) => (
        <>
          <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
          <ul className="flex flex-col gap-1">
            {r.lines.map((l) => (
              <li key={l.line_id} className="flex justify-between gap-3">
                <span>{lineName(bill, l.line_id)}</span>
                <span className="tabular-nums">{showMoney(l.extension, r.currency, locale)}</span>
              </li>
            ))}
          </ul>
          <Fact label={t("payables.net")} value={showMoney(r.net, r.currency, locale)} />
          <Fact label={t("payables.tax")} value={showMoney(r.tax, r.currency, locale)} />
          <p className="flex justify-between border-t border-(--hair) pt-2 font-semibold">
            <span>{t("payables.total")}</span>
            <span className="tabular-nums">{showMoney(r.total, r.currency, locale)}</span>
          </p>
          {r.functional_total ? (
            <Fact
              label={t("payables.functionalTotal")}
              value={showMoney(r.functional_total, r.functional_currency, locale)}
            />
          ) : null}
        </>
      )}
      commit={(fp, client_ref, request) =>
        supplierBillsPost(
          ws,
          bill.id,
          { client_ref, expected_version: bill.version, payload: { review_fingerprint: fp } },
          request
        )
      }
    />
  )
}

/** Reverse a posted bill: date + reason, the server's review, then the typed reversal. */
export function ReverseBillDialog({ bill, button, outcome }: Props) {
  const t = useTranslations()
  const bookToday = useBookToday()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const inputs = useReverseInputs("bill-rev")
  return (
    <ReviewDialog
      button={button}
      outcome={outcome}
      trigger={t("payables.reverse")}
      title={t("payables.bills.reverseTitle", { number: bill.number })}
      description={t("payables.bills.reverseText")}
      done={t("payables.done.reversed")}
      onOpen={() => inputs.reset(bookToday)}
      ready={inputs.ready}
      fields={inputs.fields}
      review={async () => (await supplierBillsReverseReview(ws, bill.id, inputs.body)).data}
      renderReview={(r) => (
        <>
          <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
          <Fact label={t("payables.reason")} value={r.reason} />
        </>
      )}
      commit={(fp, client_ref, request) =>
        supplierBillsReverse(
          ws,
          bill.id,
          { client_ref, expected_version: bill.version, payload: { ...inputs.body, review_fingerprint: fp } },
          request
        )
      }
    />
  )
}

/**
 * Split a posted bill's pending cost difference over its matched receipt lines (stock value
 * and cost of goods sold). Creates a draft; the server checks sign and size, posting reviews.
 */
export function CostAdjustDialog({ bill, button, outcome }: Props) {
  const t = useTranslations()
  const reasons = usePayReasons()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const receiptLines = [
    ...new Set(bill.lines.flatMap((l) => l.matches.map((m) => m.receipt_line_id)).filter(Boolean)),
  ] as string[]
  const [split, setSplit] = React.useState<Record<string, { inv: string; cogs: string }>>({})
  const [reason, setReason] = React.useState("")
  const [evidence, setEvidence] = React.useState("")
  const [error, setError] = React.useState<unknown>(null)
  const [busy, setBusy] = React.useState(false)
  const errors = fieldErrors(error)
  const name = (rl: string) =>
    bill.lines.find((l) => l.matches.some((m) => m.receipt_line_id === rl))?.product_name ?? ""
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        setSplit({})
        setReason("")
        setEvidence("")
        setError(null)
      }}
      button={button}
      trigger={t("payables.costs.new")}
      title={t("payables.costs.newTitle", { number: bill.number })}
      description={t("payables.costs.newText")}
    >
      <div className="flex flex-col gap-3">
        {receiptLines.map((rl, i) => (
          <fieldset key={rl} className="grid gap-2 sm:grid-cols-2">
            <legend className="text-sm font-semibold">{name(rl)}</legend>
            <Field
              id={`ca-${rl}-inv`}
              label={t("payables.costs.inventory")}
              errors={errors[`payload.allocations.${i}.inventory_amount`]}
            >
              <Input
                inputMode="decimal"
                value={split[rl]?.inv ?? ""}
                onChange={(e) => setSplit((s) => ({ ...s, [rl]: { inv: e.target.value, cogs: s[rl]?.cogs ?? "" } }))}
              />
            </Field>
            <Field
              id={`ca-${rl}-cogs`}
              label={t("payables.costs.cogs")}
              errors={errors[`payload.allocations.${i}.cogs_amount`]}
            >
              <Input
                inputMode="decimal"
                value={split[rl]?.cogs ?? ""}
                onChange={(e) => setSplit((s) => ({ ...s, [rl]: { inv: s[rl]?.inv ?? "", cogs: e.target.value } }))}
              />
            </Field>
          </fieldset>
        ))}
        <Field id="ca-reason" label={t("payables.reason")} errors={errors["payload.reason"]}>
          <Input required maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} />
        </Field>
        <Field id="ca-evidence" label={t("payables.evidenceRef")} errors={errors["payload.evidence_ref"]}>
          <Input required maxLength={255} value={evidence} onChange={(e) => setEvidence(e.target.value)} />
        </Field>
      </div>
      <ApiError error={error} fallback={t("common.error")} codes={reasons} />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>{t("actions.cancel")}</DialogClose>
        <Button
          disabled={busy || !reason.trim() || !evidence.trim()}
          onClick={async () => {
            setBusy(true)
            setError(null)
            try {
              await purchaseCostAdjustmentsCreate(
                ws,
                {
                  client_ref: a.clientRef,
                  payload: {
                    bill_id: bill.id,
                    reason: reason.trim(),
                    evidence_ref: evidence.trim(),
                    allocations: receiptLines.map((rl) => ({
                      receipt_line_id: rl,
                      inventory_amount: split[rl]?.inv.trim() || "0",
                      cogs_amount: split[rl]?.cogs.trim() || "0",
                    })),
                  },
                },
                a.key.request
              )
              a.key.settle(null)
              a.setOpen(false)
              outcome.done(t("payables.done.costCreated"))
            } catch (e) {
              a.key.settle(e)
              setError(e)
            } finally {
              setBusy(false)
            }
          }}
        >
          {t("payables.costs.save")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/** Post or reverse one cost adjustment through the server's review. */
export function CostActionDialog({
  adj,
  action,
  button,
  outcome,
}: {
  adj: CostAdjustmentResource
  action: "post" | "reverse"
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const bookToday = useBookToday()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const inputs = useReverseInputs(`ca-${adj.id}`)
  const fc = adj.functional_currency
  if (action === "post")
    return (
      <ReviewDialog
        auto
        button={button}
        outcome={outcome}
        trigger={t("payables.costs.post")}
        title={t("payables.costs.postTitle")}
        description={t("payables.costs.postText")}
        done={t("payables.done.posted")}
        review={async () => (await purchaseCostAdjustmentsPostReview(ws, adj.id)).data}
        renderReview={(r) => (
          <>
            <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
            <Fact label={t("payables.costs.pending")} value={showMoney(r.bill_pending_variance, fc, locale)} />
            <Fact label={t("payables.total")} value={showMoney(r.total, fc, locale)} />
          </>
        )}
        commit={(fp, client_ref, request) =>
          purchaseCostAdjustmentsPost(
            ws,
            adj.id,
            { client_ref, expected_version: adj.version, payload: { review_fingerprint: fp } },
            request
          )
        }
      />
    )
  return (
    <ReviewDialog
      button={button}
      outcome={outcome}
      trigger={t("payables.reverse")}
      title={t("payables.costs.reverseTitle")}
      description={t("payables.costs.reverseText")}
      done={t("payables.done.reversed")}
      onOpen={() => inputs.reset(bookToday)}
      ready={inputs.ready}
      fields={inputs.fields}
      review={async () => (await purchaseCostAdjustmentsReverseReview(ws, adj.id, inputs.body)).data}
      renderReview={(r) => (
        <>
          <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
          <Fact label={t("payables.reason")} value={r.reason} />
        </>
      )}
      commit={(fp, client_ref, request) =>
        purchaseCostAdjustmentsReverse(
          ws,
          adj.id,
          { client_ref, expected_version: adj.version, payload: { ...inputs.body, review_fingerprint: fp } },
          request
        )
      }
    />
  )
}

