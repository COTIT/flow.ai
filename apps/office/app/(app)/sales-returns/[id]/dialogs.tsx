"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useLegacyReturnCostsCreate } from "@genesy/api-client/endpoints/legacy-return-costs"
import {
  useSalesReturnRestocksReverse,
  useSalesReturnRestocksReverseReview,
} from "@genesy/api-client/endpoints/sales-return-restocks"
import {
  useSalesReturnsCancel,
  useSalesReturnsPost,
  useSalesReturnsPostReview,
  useSalesReturnsRestock,
  useSalesReturnsRestockReview,
  useSalesReturnsReverse,
  useSalesReturnsReverseReview,
  useSalesReturnsUpdate,
} from "@genesy/api-client/endpoints/sales-returns"
import type {
  RestockResource,
  SalesReturnResource,
} from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatQty } from "@genesy/ui/lib/format"
import { Field } from "@/components/field"

import {
  bookAmount,
  Shell,
  useAction,
  useBookToday,
  useReturnReasons,
  type Outcome,
} from "../shared"

type Line = SalesReturnResource["lines"][number]

type Props = {
  ret: SalesReturnResource
  outcome: Outcome
  button: React.ReactElement
}

/** A line's label: the product name from the API, else a neutral word. */
export function useLineLabel() {
  const t = useTranslations("salesReturns")
  return (l: Line) => l.product_name ?? t("line")
}

/** Reviewing… placeholder while the server prepares its review. */
function Reviewing({ label }: { label: string }) {
  return (
    <div aria-busy="true" className="flex flex-col gap-2">
      <span className="sr-only">{label}</span>
      <Skeleton className="h-4 w-2/3" />
      <Skeleton className="h-4 w-1/2" />
    </div>
  )
}

/** One label–value row of a review. */
function Row({ label, value }: { label: string; value: string }) {
  return (
    <p className="flex flex-wrap justify-between gap-x-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-medium tabular-nums">{value}</span>
    </p>
  )
}

/** Edit a draft: date, reason and the quantity per line (the API re-checks what is left). */
export function EditDialog({ ret, outcome, button }: Props) {
  const t = useTranslations()
  const reasons = useReturnReasons()
  const label = useLineLabel()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const update = useSalesReturnsUpdate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("salesReturns.done.updated"))
      },
    },
  })
  const errors = fieldErrors(update.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        update.reset()
      }}
      button={button}
      trigger={t("salesReturns.edit.action")}
      title={t("salesReturns.edit.title")}
      description={t("salesReturns.edit.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          update.mutate({
            workspace: workspaceId,
            _return: ret.id,
            data: {
              client_ref: a.clientRef,
              expected_version: ret.version,
              payload: {
                accounting_date: String(f.get("accounting_date") ?? ""),
                reason: String(f.get("reason") ?? "").trim(),
                lines: ret.lines.map((l) => ({
                  recognition_line_id: l.recognition_line_id,
                  quantity: String(f.get(`q-${l.id}`) ?? "").trim(),
                })),
              },
            },
          })
        }}
      >
        <Field
          id="sr-edit-date"
          label={t("salesReturns.date")}
          errors={errors["payload.accounting_date"]}
        >
          <Input
            type="date"
            name="accounting_date"
            required
            defaultValue={ret.accounting_date}
          />
        </Field>
        <Field
          id="sr-edit-reason"
          label={t("salesReturns.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea
            name="reason"
            required
            maxLength={1000}
            defaultValue={ret.reason}
          />
        </Field>
        {ret.lines.map((l, i) => (
          <Field
            key={l.id}
            id={`sr-edit-q-${l.id}`}
            label={t("salesReturns.quantityOf", { item: label(l) })}
            errors={errors[`payload.lines.${i}.quantity`]}
          >
            <Input
              name={`q-${l.id}`}
              inputMode="decimal"
              required
              defaultValue={l.quantity}
            />
          </Field>
        ))}
        <ApiError
          error={update.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={update.isPending}>
            {t("salesReturns.edit.save")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Cancel a draft: frees the credited quantity, nothing is booked. */
export function CancelDialog({ ret, outcome, button }: Props) {
  const t = useTranslations()
  const reasons = useReturnReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const cancel = useSalesReturnsCancel({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("salesReturns.done.cancelled"))
      },
    },
  })
  const errors = fieldErrors(cancel.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        cancel.reset()
      }}
      button={button}
      trigger={t("salesReturns.cancel.action")}
      title={t("salesReturns.cancel.title")}
      description={t("salesReturns.cancel.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          cancel.mutate({
            workspace: workspaceId,
            _return: ret.id,
            data: {
              client_ref: a.clientRef,
              expected_version: ret.version,
              payload: { reason: String(f.get("reason") ?? "").trim() },
            },
          })
        }}
      >
        <Field
          id="sr-cancel-reason"
          label={t("salesReturns.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError
          error={cancel.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("salesReturns.cancel.keep")}
          </DialogClose>
          <Button
            type="submit"
            variant="destructive"
            disabled={cancel.isPending}
          >
            {t("salesReturns.cancel.action")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Post: the server's review (net, tax, rounding, credit, applied) first, then its fingerprint. */
export function PostDialog({ ret, outcome, button }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReturnReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const review = useSalesReturnsPostReview()
  const post = useSalesReturnsPost({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("salesReturns.done.posted"))
      },
    },
  })
  const r = review.data?.data
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        post.reset()
        review.reset()
        review.mutate({ workspace: workspaceId, _return: ret.id })
      }}
      button={button}
      trigger={t("salesReturns.post.action")}
      title={t("salesReturns.post.title")}
      description={t("salesReturns.post.text")}
    >
      {review.isPending ? (
        <Reviewing label={t("salesReturns.reviewing")} />
      ) : r ? (
        <div className="flex flex-col gap-1 text-[13px]">
          <Row
            label={t("salesReturns.net")}
            value={bookAmount(r.net, locale)}
          />
          <Row
            label={t("salesReturns.tax")}
            value={bookAmount(r.tax, locale)}
          />
          <Row
            label={t("salesReturns.rounding")}
            value={bookAmount(r.rounding, locale)}
          />
          <Row
            label={t("salesReturns.credit")}
            value={bookAmount(r.credit, locale)}
          />
          <Row
            label={t("salesReturns.applied")}
            value={bookAmount(r.applied, locale)}
          />
        </div>
      ) : null}
      <ApiError
        error={review.error ?? post.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>
          {t("actions.cancel")}
        </DialogClose>
        <Button
          disabled={!r || post.isPending}
          onClick={() =>
            r &&
            post.mutate({
              workspace: workspaceId,
              _return: ret.id,
              data: {
                client_ref: a.clientRef,
                expected_version: ret.version,
                payload: { review_fingerprint: r.review_fingerprint },
              },
            })
          }
        >
          {t("salesReturns.post.action")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/**
 * Restock: quantity per line back into the warehouse the sale left (the API names it), the
 * server's restored value, then its fingerprint. A reviewed legacy cost is sent when recorded.
 */
export function RestockDialog({
  ret,
  outcome,
  button,
  legacy,
}: Props & { legacy: Record<string, string> }) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReturnReasons()
  const label = useLineLabel()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  // Lines that moved stock; the API refuses more than is left to put back.
  const open = ret.lines.filter((l) => l.origin_warehouse_id)
  const [qty, setQty] = React.useState<Record<string, string>>({})
  const review = useSalesReturnsRestockReview()
  const restock = useSalesReturnsRestock({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("salesReturns.done.restocked"))
      },
    },
  })
  const lines = open
    .filter((l) => (qty[l.id] ?? "").trim() !== "")
    .map((l) => ({
      return_line_id: l.id,
      quantity: (qty[l.id] ?? "").trim(),
      destination_warehouse_id: l.origin_warehouse_id ?? "",
      legacy_cost_evidence_id: legacy[l.id] ?? null,
    }))
  const r = review.data?.data
  const errors = fieldErrors(review.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        setQty({})
        review.reset()
        restock.reset()
      }}
      button={button}
      trigger={t("salesReturns.restock.action")}
      title={t("salesReturns.restock.title")}
      description={t("salesReturns.restock.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          restock.reset()
          review.mutate({
            workspace: workspaceId,
            _return: ret.id,
            data: { lines },
          })
        }}
      >
        {open.map((l) => {
          const i = lines.findIndex((x) => x.return_line_id === l.id)
          return (
            <Field
              key={l.id}
              id={`sr-restock-${l.id}`}
              label={t("salesReturns.restock.lineLabel", {
                item: label(l),
                warehouse: l.origin_warehouse_name ?? "",
                restocked: formatQty(l.restocked, locale),
                quantity: formatQty(l.quantity, locale),
              })}
              errors={
                i >= 0
                  ? [
                      ...(errors[`lines.${i}.quantity`] ?? []),
                      ...(errors[`lines.${i}.return_line_id`] ?? []),
                      ...(errors[`lines.${i}.destination_warehouse_id`] ?? []),
                    ]
                  : undefined
              }
            >
              <Input
                inputMode="decimal"
                value={qty[l.id] ?? ""}
                onChange={(ev) => {
                  setQty((all) => ({ ...all, [l.id]: ev.target.value }))
                  review.reset()
                }}
              />
            </Field>
          )
        })}
        {review.isPending ? (
          <Reviewing label={t("salesReturns.reviewing")} />
        ) : r ? (
          <p className="text-[13px] text-muted-foreground">
            {r.value != null
              ? t("salesReturns.restock.valueIs", {
                  value: bookAmount(r.value, locale),
                })
              : t("salesReturns.restock.valueAtPost")}
          </p>
        ) : null}
        <ApiError
          error={review.error ?? restock.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          {r ? (
            <Button
              type="button"
              disabled={restock.isPending}
              onClick={() =>
                restock.mutate({
                  workspace: workspaceId,
                  _return: ret.id,
                  data: {
                    client_ref: a.clientRef,
                    expected_version: ret.version,
                    payload: {
                      lines,
                      review_fingerprint: r.review_fingerprint,
                    },
                  },
                })
              }
            >
              {t("salesReturns.restock.action")}
            </Button>
          ) : (
            <Button
              type="submit"
              disabled={lines.length === 0 || review.isPending}
            >
              {t("salesReturns.review")}
            </Button>
          )}
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Reverse a posted credit: a date and reason, the server's review, then its fingerprint. */
export function ReverseDialog({ ret, outcome, button }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReturnReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const bookToday = useBookToday()
  const [date, setDate] = React.useState(bookToday)
  const [reason, setReason] = React.useState("")
  const review = useSalesReturnsReverseReview()
  const reverse = useSalesReturnsReverse({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("salesReturns.done.reversed"))
      },
    },
  })
  const r = review.data?.data
  const errors = fieldErrors(review.error ?? reverse.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        setDate(bookToday)
        setReason("")
        review.reset()
        reverse.reset()
      }}
      button={button}
      trigger={t("salesReturns.reverse.action")}
      title={t("salesReturns.reverse.title")}
      description={t("salesReturns.reverse.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          reverse.reset()
          review.mutate({
            workspace: workspaceId,
            _return: ret.id,
            data: { accounting_date: date },
          })
        }}
      >
        <Field
          id="sr-reverse-date"
          label={t("salesReturns.date")}
          errors={errors.accounting_date ?? errors["payload.accounting_date"]}
        >
          <Input
            type="date"
            required
            value={date}
            onChange={(e) => {
              setDate(e.target.value)
              review.reset()
            }}
          />
        </Field>
        <Field
          id="sr-reverse-reason"
          label={t("salesReturns.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea
            required
            maxLength={1000}
            value={reason}
            onChange={(e) => setReason(e.target.value)}
          />
        </Field>
        {review.isPending ? (
          <Reviewing label={t("salesReturns.reviewing")} />
        ) : r ? (
          <div className="flex flex-col gap-1 text-[13px]">
            <Row
              label={t("salesReturns.credit")}
              value={r.value != null ? bookAmount(r.value, locale) : "–"}
            />
            <p className="text-muted-foreground">
              {t("salesReturns.reverse.keepsRestocks")}
            </p>
          </div>
        ) : null}
        <ApiError
          error={review.error ?? reverse.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          {r ? (
            <Button
              type="button"
              variant="destructive"
              disabled={reverse.isPending || reason.trim() === ""}
              onClick={() =>
                reverse.mutate({
                  workspace: workspaceId,
                  _return: ret.id,
                  data: {
                    client_ref: a.clientRef,
                    expected_version: ret.version,
                    payload: {
                      accounting_date: date,
                      reason: reason.trim(),
                      review_fingerprint: r.review_fingerprint,
                    },
                  },
                })
              }
            >
              {t("salesReturns.reverse.action")}
            </Button>
          ) : (
            <Button type="submit" disabled={review.isPending}>
              {t("salesReturns.review")}
            </Button>
          )}
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Undo one posted restock exactly: the server's review, a reason, then its fingerprint. */
export function RestockReverseDialog({
  ret,
  restock,
  outcome,
  button,
}: Props & { restock: RestockResource }) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReturnReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [reason, setReason] = React.useState("")
  const review = useSalesReturnRestocksReverseReview()
  const reverse = useSalesReturnRestocksReverse({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("salesReturns.done.unrestocked"))
      },
    },
  })
  const r = review.data?.data
  const errors = fieldErrors(reverse.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        setReason("")
        reverse.reset()
        review.reset()
        review.mutate({
          workspace: workspaceId,
          _return: ret.id,
          restock: restock.id,
        })
      }}
      button={button}
      trigger={t("salesReturns.unrestock.action")}
      title={t("salesReturns.unrestock.title")}
      description={t("salesReturns.unrestock.text", {
        warehouse: restock.warehouse_name,
      })}
    >
      {review.isPending ? (
        <Reviewing label={t("salesReturns.reviewing")} />
      ) : r ? (
        <div className="flex flex-col gap-1 text-[13px]">
          <Row
            label={t("salesReturns.restock.value")}
            value={r.value != null ? bookAmount(r.value, locale) : "–"}
          />
        </div>
      ) : null}
      <Field
        id={`sr-unrestock-reason-${restock.id}`}
        label={t("salesReturns.reason")}
        errors={errors["payload.reason"]}
      >
        <Textarea
          required
          maxLength={1000}
          value={reason}
          onChange={(e) => setReason(e.target.value)}
        />
      </Field>
      <ApiError
        error={review.error ?? reverse.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>
          {t("actions.cancel")}
        </DialogClose>
        <Button
          variant="destructive"
          disabled={!r || reverse.isPending || reason.trim() === ""}
          onClick={() =>
            r &&
            reverse.mutate({
              workspace: workspaceId,
              _return: ret.id,
              restock: restock.id,
              data: {
                client_ref: a.clientRef,
                expected_version: ret.version,
                payload: {
                  reason: reason.trim(),
                  review_fingerprint: r.review_fingerprint,
                },
              },
            })
          }
        >
          {t("salesReturns.unrestock.action")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/**
 * Record a reviewed cost for a line whose sale has no booked cost (the API asked for it):
 * quantity, amount in book currency, evidence and reason. Restock then uses it.
 */
export function LegacyCostDialog({
  ret,
  line,
  outcome,
  button,
  onRecorded,
}: Props & { line: Line; onRecorded: (id: string) => void }) {
  const t = useTranslations()
  const reasons = useReturnReasons()
  const label = useLineLabel()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const create = useLegacyReturnCostsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        onRecorded(r.data.id)
        outcome.done(t("salesReturns.done.legacyCost"))
      },
    },
  })
  const errors = fieldErrors(create.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        create.reset()
      }}
      button={button}
      trigger={t("salesReturns.legacy.action")}
      title={t("salesReturns.legacy.title", { item: label(line) })}
      description={t("salesReturns.legacy.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: {
                original_sale_id: ret.order_id ?? "",
                line_id: line.order_line_id ?? "",
                target_product_id: line.product_id ?? null,
                quantity: String(f.get("quantity") ?? "").trim(),
                restoration_amount: String(f.get("amount") ?? "").trim(),
                evidence_ref: String(f.get("evidence") ?? "").trim(),
                reason: String(f.get("reason") ?? "").trim(),
              },
            },
          })
        }}
      >
        <Field
          id={`sr-legacy-qty-${line.id}`}
          label={t("salesReturns.quantity")}
          errors={errors["payload.quantity"]}
        >
          <Input
            name="quantity"
            inputMode="decimal"
            required
            defaultValue={line.quantity}
          />
        </Field>
        <Field
          id={`sr-legacy-amount-${line.id}`}
          label={t("salesReturns.legacy.amount")}
          errors={errors["payload.restoration_amount"]}
        >
          <Input name="amount" inputMode="decimal" required />
        </Field>
        <Field
          id={`sr-legacy-evidence-${line.id}`}
          label={t("salesReturns.legacy.evidence")}
          errors={errors["payload.evidence_ref"]}
        >
          <Input name="evidence" required maxLength={255} />
        </Field>
        <Field
          id={`sr-legacy-reason-${line.id}`}
          label={t("salesReturns.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError
          error={create.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={create.isPending}>
            {t("salesReturns.legacy.save")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}
