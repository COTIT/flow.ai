"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  usePurchaseOrdersCancel,
  usePurchaseOrdersCloseLines,
  usePurchaseOrdersConfirm,
  usePurchaseOrdersConfirmReview,
} from "@genesy/api-client/endpoints/purchase-orders"
import {
  usePurchaseReceiptsCreate,
} from "@genesy/api-client/endpoints/purchase-receipts"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import type { PurchaseOrderResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatQty } from "@genesy/ui/lib/format"
import { Field, selectClass } from "@/components/field"
import { useIdempotencyKey } from "@/lib/idempotency"

import { isStale, showMoney, useReasons } from "../shared"

export type Outcome = {
  /** A success sentence for the page's Done notice; the dialog has closed. */
  done: (message: string) => void
  /** The API said the order changed (409 stale_document): reload it and say so. */
  stale: () => void
}

/** One action's dialog: its key per attempt, its open state, its stale handling. */
export function useAction(outcome: Outcome) {
  const [open, setOpen] = React.useState(false)
  const key = useIdempotencyKey()
  const options = {
    mutation: {
      onSuccess: () => setOpen(false),
      onError: (e: unknown) => {
        if (isStale(e)) {
          setOpen(false)
          outcome.stale()
        }
      },
      onSettled: (_d: unknown, e: unknown) => key.settle(e),
    },
    request: key.request,
  }
  const clientRef = key.request.headers["Idempotency-Key"]
  return { open, setOpen, key, options, clientRef }
}

export function Shell({
  open,
  setOpen,
  onOpen,
  button,
  trigger,
  title,
  description,
  children,
}: {
  open: boolean
  setOpen: (o: boolean) => void
  onOpen?: () => void
  button: React.ReactElement
  trigger: string
  title: string
  description: string
  children: React.ReactNode
}) {
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) onOpen?.()
      }}
    >
      <DialogTrigger render={button}>{trigger}</DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {children}
      </DialogContent>
    </Dialog>
  )
}

/** Confirm: the server's review (frozen supplier, lines, total) first, then its fingerprint back. */
export function ConfirmPoDialog({
  order,
  button,
  outcome,
}: {
  order: PurchaseOrderResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const review = usePurchaseOrdersConfirmReview()
  const confirm = usePurchaseOrdersConfirm({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.purchase.confirmed"))
      },
    },
  })
  const name = (lineId: string) =>
    order.lines.find((l) => l.id === lineId)?.product.name ?? ""
  const r = review.data?.data
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        confirm.reset()
        review.mutate({ workspace: workspaceId, order: order.id })
      }}
      button={button}
      trigger={t("purchases.confirm")}
      title={t("purchases.confirmTitle", { number: order.number })}
      description={t("purchases.confirmText")}
    >
      {review.isPending ? (
        <div aria-busy="true" className="flex flex-col gap-2">
          <span className="sr-only">{t("purchases.reviewing")}</span>
          <Skeleton className="h-4 w-2/3" />
          <Skeleton className="h-4 w-1/2" />
        </div>
      ) : r ? (
        <div className="flex flex-col gap-2 text-[13px]">
          <p className="font-semibold">
            {(r.supplier_snapshot as { company_name?: string } | null)?.company_name ??
              order.supplier.name}
          </p>
          <ul className="flex flex-col gap-1">
            {r.lines.map((l) => (
              <li key={l.line_id} className="flex justify-between gap-3">
                <span>
                  {name(l.line_id)} × {formatQty(l.base_quantity, locale)}
                </span>
                <span className="tabular-nums">{showMoney(l.extension, r.currency, locale)}</span>
              </li>
            ))}
          </ul>
          <p className="flex justify-between border-t border-(--hair) pt-2 font-semibold">
            <span>{t("purchases.totalOrdered")}</span>
            <span className="tabular-nums">{showMoney(r.totals.ordered, r.currency, locale)}</span>
          </p>
        </div>
      ) : null}
      <ApiError error={review.error ?? confirm.error} fallback={t("common.error")} codes={reasons} />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>
          {t("actions.cancel")}
        </DialogClose>
        <Button
          disabled={!r || confirm.isPending}
          onClick={() =>
            r &&
            confirm.mutate({
              workspace: workspaceId,
              order: order.id,
              data: {
                client_ref: a.clientRef,
                expected_version: order.version,
                payload: { review_fingerprint: r.review_fingerprint },
              },
            })
          }
        >
          {t("purchases.confirm")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/** Receive: what arrived per open line; empty lines are not sent. */
export function ReceiveDialog({
  order,
  button,
  outcome,
}: {
  order: PurchaseOrderResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const warehouses = useWarehousesList(
    workspaceId,
    { page_size: 100 },
    { query: { enabled: a.open && Boolean(workspaceId) } }
  )
  const receive = usePurchaseReceiptsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        outcome.done(t("done.purchase.received", { number: r.data.number }))
      },
    },
  })
  const open = order.lines.filter((l) => Number(l.open_quantity) > 0)
  // Error keys follow the sent lines, which skip the empty ones.
  const [sent, setSent] = React.useState<string[]>([])
  const errors = fieldErrors(receive.error)
  const lineErr = (id: string) => {
    const i = sent.indexOf(id)
    return i < 0
      ? undefined
      : (errors[`payload.lines.${i}.base_quantity`] ??
          errors[`payload.lines.${i}.warehouse_id`] ??
          errors[`payload.lines.${i}.lot_id`] ??
          errors[`payload.lines.${i}`])
  }
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        receive.reset()
      }}
      button={button}
      trigger={t("purchases.receive")}
      title={t("purchases.receiveTitle", { number: order.number })}
      description={t("purchases.receiveText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          const text = (k: string) => String(f.get(k) ?? "").trim()
          const lines = open
            .filter((l) => text(`q-${l.id}`) !== "")
            .map((l) => ({
              po_line_id: l.id,
              base_quantity: text(`q-${l.id}`),
              warehouse_id: text(`w-${l.id}`),
            }))
          setSent(lines.map((l) => l.po_line_id))
          receive.mutate({
            workspace: workspaceId,
            order: order.id,
            data: {
              client_ref: a.clientRef,
              payload: {
                effective_receipt_date: text("effective_receipt_date"),
                note: text("note") || null,
                lines,
              },
            },
          })
        }}
      >
        <Field
          id="rc-date"
          label={t("purchases.effectiveDate")}
          errors={errors["payload.effective_receipt_date"]}
        >
          <Input type="date" name="effective_receipt_date" required defaultValue={order.business_date} max={order.business_date} />
        </Field>
        {open.map((l) => (
          <fieldset key={l.id} className="grid gap-2 sm:grid-cols-2">
            <legend className="sr-only">{l.product.name}</legend>
            <Field
              id={`rc-q-${l.id}`}
              label={t("purchases.openOf", {
                product: l.product.name,
                open: formatQty(l.open_quantity, locale),
              })}
              errors={lineErr(l.id)}
            >
              <Input name={`q-${l.id}`} inputMode="decimal" />
            </Field>
            <Field id={`rc-w-${l.id}`} label={t("purchases.warehouse")}>
              <select
                // Remount when the list arrives so the order's warehouse preselects.
                key={warehouses.data ? "ready" : "loading"}
                name={`w-${l.id}`}
                className={selectClass}
                defaultValue={l.warehouse_id ?? order.default_warehouse_id ?? ""}
                required
              >
                <option value="" disabled>
                  {t("purchases.noWarehouse")}
                </option>
                {warehouses.data?.data.map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.name}
                  </option>
                ))}
              </select>
            </Field>
          </fieldset>
        ))}
        <Field id="rc-note" label={t("purchases.note")} errors={errors["payload.note"]}>
          <Textarea name="note" maxLength={1000} />
        </Field>
        {errors["payload.lines"]?.length ? (
          <Notice variant="error" title={errors["payload.lines"].join(" ")} />
        ) : null}
        <ApiError error={receive.error} fallback={t("common.error")} codes={reasons} />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={receive.isPending}>
            {t("purchases.receive")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Close the open remainder of chosen lines, with a reason; received goods stay bookable. */
export function CloseDialog({
  order,
  button,
  outcome,
}: {
  order: PurchaseOrderResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const close = usePurchaseOrdersCloseLines({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.purchase.closed"))
      },
    },
  })
  const open = order.lines.filter((l) => Number(l.open_quantity) > 0)
  const [sent, setSent] = React.useState<string[]>([])
  const errors = fieldErrors(close.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        close.reset()
      }}
      button={button}
      trigger={t("purchases.close")}
      title={t("purchases.closeTitle", { number: order.number })}
      description={t("purchases.closeText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          const text = (k: string) => String(f.get(k) ?? "").trim()
          const lines = open
            .filter((l) => text(`c-${l.id}`) !== "")
            .map((l) => ({ line_id: l.id, quantity: text(`c-${l.id}`) }))
          setSent(lines.map((l) => l.line_id))
          close.mutate({
            workspace: workspaceId,
            order: order.id,
            data: {
              client_ref: a.clientRef,
              expected_version: order.version,
              payload: { reason: text("reason"), lines },
            },
          })
        }}
      >
        {open.map((l) => {
          const i = sent.indexOf(l.id)
          return (
            <Field
              key={l.id}
              id={`cl-${l.id}`}
              label={t("purchases.openOf", {
                product: l.product.name,
                open: formatQty(l.open_quantity, locale),
              })}
              errors={i < 0 ? undefined : errors[`payload.lines.${i}.quantity`]}
            >
              <Input name={`c-${l.id}`} inputMode="decimal" defaultValue={l.open_quantity} />
            </Field>
          )
        })}
        <Field id="cl-reason" label={t("purchases.reason")} errors={errors["payload.reason"]}>
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError error={close.error} fallback={t("common.error")} codes={reasons} />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" variant="destructive" disabled={close.isPending}>
            {t("purchases.close")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Cancel an order nothing was received on, with a reason (OfficeStates confirm look). */
export function CancelDialog({
  order,
  button,
  outcome,
}: {
  order: PurchaseOrderResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const cancel = usePurchaseOrdersCancel({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.purchase.cancelled"))
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
      trigger={t("purchases.cancel")}
      title={t("purchases.cancelTitle", { number: order.number })}
      description={t("purchases.cancelText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const reason = String(new FormData(e.currentTarget).get("reason") ?? "").trim()
          cancel.mutate({
            workspace: workspaceId,
            order: order.id,
            data: {
              client_ref: a.clientRef,
              expected_version: order.version,
              payload: { reason },
            },
          })
        }}
      >
        <Field id="cn-reason" label={t("purchases.reason")} errors={errors["payload.reason"]}>
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError error={cancel.error} fallback={t("common.error")} codes={reasons} />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" variant="destructive" disabled={cancel.isPending}>
            {t("purchases.cancel")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}
