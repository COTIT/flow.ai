"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"
import { Trash2 } from "lucide-react"

import { useSession } from "@verzo/api-client/auth"
import { useCurrenciesList } from "@verzo/api-client/endpoints/currencies"
import {
  usePurchaseReceiptsBook,
  usePurchaseReceiptsBookReview,
} from "@verzo/api-client/endpoints/purchase-receipts"
import type {
  LandedCostPayload,
  PurchaseOrderResource,
  PurchaseReceiptResource,
} from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Checkbox } from "@verzo/ui/components/checkbox"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Label } from "@verzo/ui/components/label"
import { Notice } from "@verzo/ui/components/notice"
import { formatDay, formatQty } from "@verzo/ui/lib/format"
import { Field, selectClass } from "@/components/field"
import { useIdempotencyKey } from "@/lib/idempotency"

import { isStale, showMoney, useReasons } from "../../../shared"
import type { Outcome } from "../../dialogs"

type Cost = { ref: string; amount: string; currency_id: string; evidence_ref: string }

/**
 * Book: choose pending lines and extra costs, read the server's review (values, rate, journal,
 * or "stock only" in a setup book), then book with its fingerprint. Changing a choice drops the
 * review. Each cost row keeps its client_component_ref for the dialog's life, so a retry can't
 * add the same freight twice.
 */
export function BookDialog({
  order,
  receipt,
  name,
  button,
  outcome,
}: {
  order: PurchaseOrderResource
  receipt: PurchaseReceiptResource
  name: (receiptLineId: string) => string
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const pending = receipt.lines.filter((l) => l.state === "pending")
  const [chosen, setChosen] = React.useState<string[]>([])
  const [costs, setCosts] = React.useState<Cost[]>([])
  const currencies = useCurrenciesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: open && Boolean(workspaceId) } }
  )
  const key = useIdempotencyKey()
  const review = usePurchaseReceiptsBookReview()
  const book = usePurchaseReceiptsBook({
    mutation: {
      onSuccess: () => {
        setOpen(false)
        outcome.done(t("done.purchase.booked"))
      },
      onError: (e) => {
        if (isStale(e)) {
          setOpen(false)
          outcome.stale()
        }
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  const landed: LandedCostPayload[] = costs.map((c) => ({
    client_component_ref: c.ref,
    amount: c.amount.trim(),
    currency_id: c.currency_id,
    evidence_ref: c.evidence_ref.trim() || null,
  }))
  const body = { receipt_line_ids: chosen, landed_costs: landed }
  const vars = { workspace: workspaceId, order: order.id, receipt: receipt.id }
  const changed = () => {
    review.reset()
    book.reset()
  }
  const r = review.data?.data
  const errors = fieldErrors(review.error ?? book.error)
  const costErr = (i: number) =>
    errors[`payload.landed_costs.${i}.amount`] ??
    errors[`payload.landed_costs.${i}.client_component_ref`] ??
    errors[`payload.landed_costs.${i}.explicit_line_allocations`] ??
    errors[`landed_costs.${i}.amount`] ??
    errors[`landed_costs.${i}.explicit_line_allocations`] ??
    errors[`payload.landed_costs.${i}`] ??
    errors[`landed_costs.${i}`]
  const defaultCurrency = order.currency.id

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) {
          key.renew()
          setChosen(pending.map((l) => l.id))
          setCosts([])
          changed()
        }
      }}
    >
      <DialogTrigger render={button}>{t("purchases.book")}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t("purchases.bookTitle", { number: receipt.number })}</DialogTitle>
          <DialogDescription>{t("purchases.bookText")}</DialogDescription>
        </DialogHeader>
        <fieldset className="flex flex-col gap-2" disabled={Boolean(r)}>
          <legend className="sr-only">{t("purchases.lines")}</legend>
          {pending.map((l) => (
            <Label key={l.id} className="flex items-center gap-3 font-normal">
              <Checkbox
                checked={chosen.includes(l.id)}
                onCheckedChange={(on) => {
                  setChosen((c) => (on ? [...c, l.id] : c.filter((x) => x !== l.id)))
                  changed()
                }}
              />
              <span>
                {name(l.id)} × {formatQty(l.base_quantity, locale)}
              </span>
            </Label>
          ))}
        </fieldset>
        {receipt.lines.some((l) => l.state === "late") ? (
          <Notice variant="warning" title={t("purchases.lateWarning")} />
        ) : null}

        <fieldset className="flex flex-col gap-3" disabled={Boolean(r)}>
          <legend className="text-sm font-semibold">{t("purchases.landedCosts")}</legend>
          <p className="text-[13px] text-muted-foreground">{t("purchases.landedCostsText")}</p>
          {costs.map((c, i) => (
            <div key={c.ref} className="grid gap-2 sm:grid-cols-[1fr_1fr_1.5fr_auto] sm:items-end">
              <Field id={`lc-${c.ref}-amount`} label={t("purchases.amount")} errors={costErr(i)}>
                <Input
                  inputMode="decimal"
                  required
                  value={c.amount}
                  onChange={(e) => {
                    setCosts((cs) => cs.map((x) => (x.ref === c.ref ? { ...x, amount: e.target.value } : x)))
                    changed()
                  }}
                />
              </Field>
              <Field id={`lc-${c.ref}-ccy`} label={t("purchases.currency")}>
                <select
                  className={selectClass}
                  value={c.currency_id}
                  onChange={(e) => {
                    setCosts((cs) => cs.map((x) => (x.ref === c.ref ? { ...x, currency_id: e.target.value } : x)))
                    changed()
                  }}
                >
                  <option value={order.currency.id}>{order.currency.code}</option>
                  {currencies.data?.data
                    .filter((x) => x.id !== order.currency.id)
                    .map((x) => (
                      <option key={x.id} value={x.id}>
                        {x.asset_type}
                      </option>
                    ))}
                </select>
              </Field>
              <Field id={`lc-${c.ref}-ev`} label={t("purchases.evidenceRef")}>
                <Input
                  maxLength={255}
                  value={c.evidence_ref}
                  onChange={(e) => {
                    setCosts((cs) => cs.map((x) => (x.ref === c.ref ? { ...x, evidence_ref: e.target.value } : x)))
                    changed()
                  }}
                />
              </Field>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                aria-label={t("purchases.removeCost", { n: i + 1 })}
                onClick={() => {
                  setCosts((cs) => cs.filter((x) => x.ref !== c.ref))
                  changed()
                }}
              >
                <Trash2 className="size-4" />
              </Button>
            </div>
          ))}
          <Button
            type="button"
            variant="outline"
            className="self-start"
            onClick={() => {
              setCosts((cs) => [
                ...cs,
                { ref: crypto.randomUUID(), amount: "", currency_id: defaultCurrency, evidence_ref: "" },
              ])
              changed()
            }}
          >
            {t("purchases.addCost")}
          </Button>
        </fieldset>

        {r ? (
          <section aria-labelledby="book-review" className="flex flex-col gap-2 text-[13px]">
            <h3 id="book-review" className="text-sm font-semibold">
              {t("purchases.reviewTitle")}
            </h3>
            <p className="flex justify-between">
              <span className="text-muted-foreground">{t("purchases.accountingDate")}</span>
              <span>{formatDay(r.accounting_date, locale)}</span>
            </p>
            <ul className="flex flex-col gap-1">
              {r.lines.map((l) => (
                <li key={l.receipt_line_id} className="flex flex-wrap justify-between gap-x-3">
                  <span>
                    {name(l.receipt_line_id)} × {formatQty(l.base_quantity, locale)}
                  </span>
                  {l.value != null && r.functional_currency ? (
                    <span className="tabular-nums">
                      {showMoney(l.value, r.functional_currency, locale)}
                    </span>
                  ) : null}
                </li>
              ))}
            </ul>
            {r.landed_costs.map((c) => (
              <p key={c.client_component_ref} className="flex justify-between gap-3">
                <span className="text-muted-foreground">
                  {t("purchases.landedValue")} {showMoney(c.amount, c.currency, locale)}
                </span>
                {r.functional_currency ? (
                  <span className="tabular-nums">
                    {showMoney(c.functional_amount, r.functional_currency, locale)}
                  </span>
                ) : null}
              </p>
            ))}
            {r.total != null && r.functional_currency ? (
              <p className="flex justify-between border-t border-(--hair) pt-2 font-semibold">
                <span>{t("purchases.total")}</span>
                <span className="tabular-nums">{showMoney(r.total, r.functional_currency, locale)}</span>
              </p>
            ) : null}
            {r.journal ? (
              <p className="text-xs text-muted-foreground">{t("purchases.journal")}</p>
            ) : (
              <Notice title={t("purchases.stockOnly")} />
            )}
          </section>
        ) : null}

        {errors["payload.receipt_line_ids"] ?? errors["receipt_line_ids"] ? (
          <Notice
            variant="error"
            title={(errors["payload.receipt_line_ids"] ?? errors["receipt_line_ids"])!.join(" ")}
          />
        ) : null}
        <ApiError error={review.error ?? book.error} fallback={t("common.error")} codes={reasons} />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          {r ? (
            <>
              <Button type="button" variant="outline" onClick={changed}>
                {t("purchases.changeChoice")}
              </Button>
              <Button
                disabled={book.isPending}
                onClick={() =>
                  book.mutate({
                    ...vars,
                    data: {
                      client_ref: key.request.headers["Idempotency-Key"],
                      expected_version: receipt.version,
                      payload: { ...body, review_fingerprint: r.review_fingerprint },
                    },
                  })
                }
              >
                {t("purchases.book")}
              </Button>
            </>
          ) : (
            <Button
              disabled={chosen.length === 0 || review.isPending}
              onClick={() => review.mutate({ ...vars, data: body })}
            >
              {t("purchases.review")}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
