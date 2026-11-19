"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getPurchaseReturnsListQueryKey,
  usePurchaseReturnsCandidates,
  usePurchaseReturnsCreate,
} from "@genesy/api-client/endpoints/purchase-returns"
import type { ReturnLinePayload } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Checkbox } from "@genesy/ui/components/checkbox"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"
import { Notice } from "@genesy/ui/components/notice"
import { formatQty } from "@genesy/ui/lib/format"
import { DetailSkeleton } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { usePayReasons, usePayWord } from "@/components/payables"
import { LoadError, NoAccess } from "@/components/states"
import { useCan, useRights } from "@/lib/can"
import { useIdempotencyKey } from "@/lib/idempotency"

import { today } from "../../purchases/shared"

type Pick = { source: string; quantity: string }

/**
 * Return goods of one booked receipt: per receipt line, what the server says may still go back
 * unbilled or against each posted bill match, one source and a quantity per line. Creates a
 * draft; posting reviews the stock value and supplier credit.
 */
export default function NewPurchaseReturnPage({
  searchParams,
}: {
  searchParams: Promise<{ receipt?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const word = usePayWord()
  const reasons = usePayReasons()
  const queryClient = useQueryClient()
  const rights = useRights()
  const may = useCan("purchasereturn.create")
  const { receipt } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const candidates = usePurchaseReturnsCandidates(
    workspaceId,
    { receipt_id: receipt ?? "" },
    { query: { enabled: Boolean(workspaceId && receipt) && may } }
  )
  const key = useIdempotencyKey()
  const [picks, setPicks] = React.useState<Record<string, Pick>>({})
  const [purpose, setPurpose] = React.useState("vendor_return")
  const [reason, setReason] = React.useState("")
  const [date, setDate] = React.useState(today())
  const [apply, setApply] = React.useState(false)
  const [evidence, setEvidence] = React.useState("")
  const create = usePurchaseReturnsCreate({
    mutation: {
      onSuccess: (r) => {
        void queryClient.invalidateQueries({ queryKey: getPurchaseReturnsListQueryKey(workspaceId) })
        router.push(`/purchase-returns/${r.data.id}?done=created`)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const errors = fieldErrors(create.error)
  const err = (k: string) => errors[`payload.${k}`]
  const head = (
    <PageHead
      back={{ href: "/purchase-returns", label: t("payables.returns.title") }}
      eyebrow={t("shell.buy")}
      title={t("payables.returns.newTitle")}
    />
  )
  if (rights.read && !may) return <NoAccess />
  if (!receipt)
    return (
      <>
        {head}
        <Notice variant="info" title={t("payables.returns.fromReceipt")} />
      </>
    )
  if (candidates.isPending) return <DetailSkeleton />
  if (candidates.isError)
    return <LoadError error={candidates.error} onRetry={() => void candidates.refetch()} backHref="/purchase-returns" />
  const c = candidates.data.data
  const chosen = c.lines.filter((l) => picks[l.receipt_line_id]?.source && picks[l.receipt_line_id]?.quantity.trim())
  const lines: ReturnLinePayload[] = chosen.map((l) => {
    const p = picks[l.receipt_line_id]!
    return {
      receipt_line_id: l.receipt_line_id,
      quantity: p.quantity.trim(),
      source_bill_allocation_id: p.source === "unbilled" ? null : p.source,
    }
  })
  const setPick = (line: string, patch: Partial<Pick>) =>
    setPicks((ps) => ({ ...ps, [line]: { source: "", quantity: "", ...ps[line], ...patch } }))

  return (
    <>
      {head}
      <form
        className="flex flex-col gap-5"
        onSubmit={(e) => {
          e.preventDefault()
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: key.request.headers["Idempotency-Key"],
              payload: {
                receipt_id: c.receipt_id,
                purpose,
                reason: reason.trim(),
                accounting_date: date,
                apply_credit: apply,
                supplier_credit_evidence_ref: evidence.trim() || null,
                lines,
              },
            },
          })
        }}
      >
        <Card className="gap-4 px-6 py-6 max-md:p-4">
          <p className="text-[13px]">
            {t("payables.returns.receiptOf", {
              receipt: c.receipt_number,
              order: c.purchase_order_number,
              supplier: c.supplier.name ?? "",
            })}
          </p>
          <div className="grid gap-4 md:grid-cols-2">
            <Field id="ret-purpose" label={t("payables.returns.purpose")} errors={err("purpose")}>
              <select className={selectClass} value={purpose} onChange={(e) => setPurpose(e.target.value)}>
                <option value="vendor_return">{word("purposes", "vendor_return")}</option>
                <option value="receipt_correction">{word("purposes", "receipt_correction")}</option>
              </select>
            </Field>
            <Field id="ret-date" label={t("payables.accountingDate")} errors={err("accounting_date")}>
              <Input type="date" required value={date} onChange={(e) => setDate(e.target.value)} />
            </Field>
            <Field id="ret-reason" label={t("payables.reason")} errors={err("reason")}>
              <Input required maxLength={1000} value={reason} onChange={(e) => setReason(e.target.value)} />
            </Field>
            <Field
              id="ret-evidence"
              label={t("payables.returns.creditEvidence")}
              errors={err("supplier_credit_evidence_ref")}
            >
              <Input maxLength={255} value={evidence} onChange={(e) => setEvidence(e.target.value)} />
            </Field>
          </div>
          <Label className="flex min-h-11 items-center gap-3 font-normal">
            <Checkbox checked={apply} onCheckedChange={(on) => setApply(Boolean(on))} />
            <span>{t("payables.returns.applyCredit")}</span>
          </Label>
          {err("apply_credit") ? <p className="text-sm text-destructive">{err("apply_credit")!.join(" ")}</p> : null}
        </Card>

        <Card className="gap-4 px-6 py-6 max-md:p-4" role="region" aria-labelledby="ret-lines">
          <h2 id="ret-lines" className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
            {t("payables.lines")}
          </h2>
          {err("lines")?.length ? <p className="text-sm text-destructive">{err("lines")!.join(" ")}</p> : null}
          {c.lines.length === 0 ? <Notice title={t("payables.returns.nothingToReturn")} /> : null}
          {c.lines.map((l) => {
            const i = chosen.findIndex((x) => x.receipt_line_id === l.receipt_line_id)
            const le = (f: string) => (i >= 0 ? err(`lines.${i}.${f}`) : undefined)
            const p = picks[l.receipt_line_id]
            return (
              <fieldset
                key={l.receipt_line_id}
                className="grid gap-3 border-t border-(--hair) pt-4 first-of-type:border-0 first-of-type:pt-0 md:grid-cols-[2fr_2fr_1fr] md:items-end"
              >
                <legend className="text-sm font-semibold">
                  {l.product_name} · {l.warehouse_name}{" "}
                  <span className="font-normal text-muted-foreground">
                    {t("payables.returns.booked", { qty: formatQty(l.booked, locale) })}
                  </span>
                </legend>
                <Field
                  id={`rl-${l.receipt_line_id}-source`}
                  label={t("payables.returns.source")}
                  errors={le("source_bill_allocation_id")}
                >
                  <select
                    className={selectClass}
                    value={p?.source ?? ""}
                    onChange={(e) => setPick(l.receipt_line_id, { source: e.target.value })}
                  >
                    <option value="">{t("payables.returns.skip")}</option>
                    <option value="unbilled" disabled={Number(l.unbilled_returnable) === 0}>
                      {t("payables.returns.unbilled", { qty: formatQty(l.unbilled_returnable, locale) })}
                    </option>
                    {purpose === "vendor_return"
                      ? l.billed.map((b) => (
                          <option key={b.bill_allocation_id} value={b.bill_allocation_id} disabled={Number(b.returnable) === 0}>
                            {t("payables.returns.billed", {
                              bill: b.bill_number,
                              invoice: b.supplier_invoice_number,
                              qty: formatQty(b.returnable, locale),
                            })}
                          </option>
                        ))
                      : null}
                  </select>
                </Field>
                <Field
                  id={`rl-${l.receipt_line_id}-qty`}
                  label={t("payables.quantity")}
                  errors={le("quantity") ?? le("receipt_line_id") ?? le("warehouse_id")}
                >
                  <Input
                    inputMode="decimal"
                    disabled={!p?.source}
                    value={p?.quantity ?? ""}
                    onChange={(e) => setPick(l.receipt_line_id, { quantity: e.target.value })}
                  />
                </Field>
              </fieldset>
            )
          })}
        </Card>

        <ApiError error={create.error} fallback={t("common.error")} codes={reasons} />
        <Button
          type="submit"
          size="lg"
          className="self-end max-md:w-full"
          disabled={create.isPending || lines.length === 0 || !reason.trim()}
        >
          {t("payables.saveDraft")}
        </Button>
      </form>
    </>
  )
}
