"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  getOrdersGetQueryKey,
  getOrdersListQueryKey,
  useOrdersCancel,
  useOrdersComplete,
  useOrdersConfirm,
  useOrdersGet,
  useOrdersInvoice,
  useOrdersPick,
  useOrdersRefund,
  useOrdersReturns,
} from "@genesy/api-client/endpoints/orders"
import type { OrderAction, OrderResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
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
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatMoney, toMinor } from "@genesy/ui/lib/money"
import { Field } from "@/components/field"
import { StatusBadge } from "@/components/status-badge"
import { useIdempotencyKey } from "@/lib/idempotency"

import { customerName } from "../shared"

export default function OrderPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const order = useOrdersGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  const onSuccess = () => {
    for (const queryKey of [
      getOrdersGetQueryKey(workspaceId, id),
      getOrdersListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const vars = { workspace: workspaceId, order: id }
  const confirmKey = useIdempotencyKey()
  const completeKey = useIdempotencyKey()
  const cancelKey = useIdempotencyKey()
  const invoiceKey = useIdempotencyKey()
  const confirm = useOrdersConfirm({
    mutation: { onSuccess, onSettled: (_d, e) => confirmKey.settle(e) },
    request: confirmKey.request,
  })
  const complete = useOrdersComplete({
    mutation: { onSuccess, onSettled: (_d, e) => completeKey.settle(e) },
    request: completeKey.request,
  })
  const cancel = useOrdersCancel({
    mutation: { onSuccess, onSettled: (_d, e) => cancelKey.settle(e) },
    request: cancelKey.request,
  })
  const invoice = useOrdersInvoice({
    mutation: { onSuccess, onSettled: (_d, e) => invoiceKey.settle(e) },
    request: invoiceKey.request,
  })
  const actionError =
    confirm.error ?? complete.error ?? cancel.error ?? invoice.error

  if (order.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (order.isError)
    return <ApiError error={order.error} fallback={t("common.error")} />
  const o = order.data.data
  const money = (minor: number) =>
    formatMoney(minor, o.decimals, o.currency, locale)
  const statusLabel = (s: string) =>
    t.has(`orders.statuses.${s}`) ? t(`orders.statuses.${s}`) : s
  // The API lists what this order's status and payments allow; anything else would be a 409.
  const can = (action: OrderAction) => o.allowed_actions.includes(action)

  return (
    <>
      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <h1 className="heading-2 text-lg">{o.number}</h1>
          <StatusBadge status={o.status} label={statusLabel(o.status)} />
          <span className="text-sm text-muted-foreground">{o.source}</span>
        </div>
        <dl className="grid max-w-md grid-cols-2 gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted-foreground">{t("orders.customer")}</dt>
          <dd>{customerName(o.customer) || "—"}</dd>
          <dt className="text-muted-foreground">{t("orders.reference")}</dt>
          <dd>{o.reference ?? "—"}</dd>
          <dt className="text-muted-foreground">{t("invoices.date")}</dt>
          <dd>{o.created_at.slice(0, 10)}</dd>
          <dt className="text-muted-foreground">{t("orders.paidAt")}</dt>
          <dd>{o.paid_at?.slice(0, 10) ?? "—"}</dd>
          {o.invoice_id ? (
            <>
              <dt className="text-muted-foreground">{t("orders.invoice")}</dt>
              <dd>
                <Link
                  className="underline-offset-4 hover:underline"
                  href={`/invoices/${o.invoice_id}`}
                >
                  {t("orders.openInvoice")}
                </Link>
              </dd>
            </>
          ) : null}
        </dl>

        <div className="flex flex-wrap gap-2">
          {can("confirm") ? (
            <Button
              disabled={confirm.isPending}
              onClick={() => confirm.mutate(vars)}
            >
              {t("orders.confirm")}
            </Button>
          ) : null}
          {can("pick") ? <PickDialog order={o} onDone={onSuccess} /> : null}
          {can("complete") ? (
            <ConfirmDialog
              trigger={t("orders.complete")}
              destructive={false}
              title={t("orders.completeTitle", { number: o.number })}
              description={t("orders.completeText")}
              confirm={t("orders.complete")}
              cancel={t("actions.cancel")}
              disabled={complete.isPending}
              onConfirm={() => complete.mutate(vars)}
            />
          ) : null}
          {can("invoice") ? (
            <Button
              variant="outline"
              disabled={invoice.isPending}
              onClick={() => invoice.mutate(vars)}
            >
              {t("orders.makeInvoice")}
            </Button>
          ) : null}
          {can("return") ? (
            <ReturnsDialog order={o} onDone={onSuccess} />
          ) : null}
          {can("refund") ? <RefundDialog order={o} onDone={onSuccess} /> : null}
          {can("cancel") ? (
            <ConfirmDialog
              trigger={t("orders.cancel")}
              title={t("orders.cancelTitle", { number: o.number })}
              description={t("orders.cancelText")}
              confirm={t("orders.cancel")}
              cancel={t("actions.cancel")}
              disabled={cancel.isPending}
              onConfirm={() => cancel.mutate(vars)}
            />
          ) : null}
        </div>
        <ApiError error={actionError} fallback={t("common.error")} />
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="heading-2 text-base">{t("invoices.lines")}</h2>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("invoices.item")}</TableHead>
              <TableHead className="text-right">
                {t("invoices.quantity")}
              </TableHead>
              <TableHead className="text-right">{t("orders.picked")}</TableHead>
              <TableHead className="text-right">
                {t("orders.returned")}
              </TableHead>
              <TableHead className="text-right">
                {t("orders.unitPrice")}
              </TableHead>
              <TableHead className="text-right">
                {t("orders.lineTotal")}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {o.lines.map((l) => (
              <TableRow key={l.id}>
                <TableCell>{l.title}</TableCell>
                <TableCell className="text-right tabular-nums">
                  {l.quantity}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {l.picked}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {l.returned}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {money(l.unit_price)}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {money(l.line_excl)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
        <dl className="ml-auto grid grid-cols-2 gap-x-4 gap-y-1 text-sm tabular-nums">
          <dt className="text-muted-foreground">{t("orders.subtotal")}</dt>
          <dd className="text-right">{money(o.subtotal)}</dd>
          <dt className="text-muted-foreground">{t("orders.vat")}</dt>
          <dd className="text-right">{money(o.vat)}</dd>
          <dt className="font-medium">{t("invoices.total")}</dt>
          <dd className="text-right font-medium">{money(o.total)}</dd>
        </dl>
      </section>
    </>
  )
}

/** Per-line quantities typed by staff; a dialog's Cancel sends nothing. */
function LinesDialog({
  order,
  trigger,
  title,
  description,
  initial,
  pending,
  error,
  extra,
  onSubmit,
  open,
  setOpen,
}: {
  order: OrderResource
  trigger: string
  title: string
  description: string
  initial: (line: OrderResource["lines"][number]) => string
  pending: boolean
  error: unknown
  extra?: React.ReactNode
  onSubmit: (
    lines: { line_id: string; quantity: string }[],
    form: FormData
  ) => void
  open: boolean
  setOpen: (open: boolean) => void
}) {
  const t = useTranslations()
  const errors = fieldErrors(error)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger render={<Button variant="outline" />}>
        {trigger}
      </DialogTrigger>
      <DialogContent>
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault()
            const f = new FormData(e.currentTarget)
            onSubmit(
              order.lines.map((l) => ({
                line_id: l.id,
                quantity: String(f.get(l.id) ?? "").trim(),
              })),
              f
            )
          }}
        >
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{description}</DialogDescription>
          </DialogHeader>
          {order.lines.map((l, i) => (
            <Field
              key={l.id}
              id={`line-${l.id}`}
              label={`${l.title} (${l.quantity})`}
              errors={errors[`lines.${i}.quantity`] ?? errors[`lines.${i}`]}
            >
              <Input
                name={l.id}
                inputMode="decimal"
                defaultValue={initial(l)}
              />
            </Field>
          ))}
          {extra}
          <ApiError error={error} fallback={t("common.error")} />
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              {t("actions.cancel")}
            </DialogClose>
            <Button type="submit" disabled={pending}>
              {trigger}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function PickDialog({
  order,
  onDone,
}: {
  order: OrderResource
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const key = useIdempotencyKey()
  const pick = useOrdersPick({
    mutation: {
      onSuccess: () => {
        onDone()
        setOpen(false)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  return (
    <LinesDialog
      order={order}
      trigger={t("orders.pick")}
      title={t("orders.pickTitle", { number: order.number })}
      description={t("orders.pickText")}
      // Picked is a running total per line; start from the full quantity.
      initial={(l) => l.quantity}
      pending={pick.isPending}
      error={pick.error}
      open={open}
      setOpen={(o) => {
        setOpen(o)
        if (o) key.renew()
        else pick.reset()
      }}
      onSubmit={(lines) =>
        pick.mutate({
          workspace: workspaceId,
          order: order.id,
          data: { lines },
        })
      }
    />
  )
}

function ReturnsDialog({
  order,
  onDone,
}: {
  order: OrderResource
  onDone: () => void
}) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const key = useIdempotencyKey()
  const returns = useOrdersReturns({
    mutation: {
      onSuccess: () => {
        onDone()
        setOpen(false)
      },
      onSettled: (_d, e) => key.settle(e),
    },
    request: key.request,
  })
  return (
    <LinesDialog
      order={order}
      trigger={t("orders.returns")}
      title={t("orders.returnsTitle", { number: order.number })}
      description={t("orders.returnsText")}
      initial={() => ""}
      pending={returns.isPending}
      error={returns.error}
      open={open}
      setOpen={(o) => {
        setOpen(o)
        if (o) key.renew()
        else returns.reset()
      }}
      extra={
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" name="restock" defaultChecked />
          {t("orders.restock")}
        </label>
      }
      onSubmit={(lines, f) =>
        returns.mutate({
          workspace: workspaceId,
          order: order.id,
          data: {
            // Lines left empty are not being returned.
            lines: lines.filter((l) => l.quantity !== ""),
            restock: f.get("restock") === "on",
          },
        })
      }
    />
  )
}

function RefundDialog({
  order,
  onDone,
}: {
  order: OrderResource
  onDone: () => void
}) {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const [amount, setAmount] = React.useState("")
  const key = useIdempotencyKey()
  const refund = useOrdersRefund({
    mutation: { onSuccess: onDone, onSettled: (_d, e) => key.settle(e) },
    request: key.request,
  })
  const minor = toMinor(amount, order.decimals)
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        setOpen(o)
        if (o) key.renew()
        else {
          refund.reset()
          setAmount("")
        }
      }}
    >
      <DialogTrigger render={<Button variant="destructive" />}>
        {t("orders.refund")}
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {t("orders.refundTitle", { number: order.number })}
          </DialogTitle>
          <DialogDescription>{t("orders.refundText")}</DialogDescription>
        </DialogHeader>
        {refund.data ? (
          <p role="status" className="text-sm">
            {t("orders.refundQueued", {
              amount: formatMoney(
                refund.data.data.amount,
                order.decimals,
                order.currency,
                locale
              ),
              status: refund.data.data.status,
            })}
          </p>
        ) : (
          <form
            className="flex flex-col gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              if (minor !== null)
                refund.mutate({
                  workspace: workspaceId,
                  order: order.id,
                  data: { amount: minor },
                })
            }}
          >
            <Field
              id="refund-amount"
              label={t("orders.refundAmount", { currency: order.currency })}
              errors={
                amount && minor === null
                  ? [t("orders.badAmount")]
                  : fieldErrors(refund.error).amount
              }
            >
              <Input
                inputMode="decimal"
                value={amount}
                onChange={(e) => setAmount(e.target.value)}
                required
              />
            </Field>
            <ApiError error={refund.error} fallback={t("common.error")} />
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                {t("actions.cancel")}
              </DialogClose>
              <Button
                type="submit"
                variant="destructive"
                disabled={refund.isPending || minor === null}
              >
                {t("orders.refund")}
              </Button>
            </DialogFooter>
          </form>
        )}
      </DialogContent>
    </Dialog>
  )
}
