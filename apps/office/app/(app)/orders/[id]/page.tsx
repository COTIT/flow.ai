"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"
import { ChevronRight, Package } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import {
  getOrdersEventsQueryKey,
  getOrdersGetQueryKey,
  getOrdersListQueryKey,
  useOrdersCancel,
  useOrdersComplete,
  useOrdersConfirm,
  useOrdersEvents,
  useOrdersGet,
  useOrdersInvoice,
  useOrdersPick,
  useOrdersRefund,
  useOrdersReturns,
} from "@genesy/api-client/endpoints/orders"
import type { OrderAction, OrderResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
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
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatMoney, toMinor } from "@genesy/ui/lib/money"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { useIdempotencyKey } from "@/lib/idempotency"
import { Done, LoadError, useDone } from "@/components/states"

import { customerName, initials } from "../shared"

// Display order of the actions; which of them show comes only from the API's `allowed_actions`.
const actionOrder: OrderAction[] = [
  "confirm",
  "pick",
  "complete",
  "invoice",
  "return",
  "refund",
  "cancel",
]

// Timeline badge colours by event kind (OfficeOrder); kinds not listed take the status colour.
const kindTone: Record<string, "teal" | "gold" | "red" | "slate"> = {
  created: "slate",
  confirmed: "slate",
  picked: "slate",
  invoiced: "slate",
  expired: "red",
  returned: "gold",
  refunded: "gold",
}

/** The order detail (OfficeOrder): lines with the server's totals, next step, customer, timeline. */
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
  // shortcut: the newest page of the trail only; page it when orders get that many steps.
  const trail = useOrdersEvents(workspaceId, id, undefined, {
    query: { enabled: Boolean(workspaceId) },
  })
  const onSuccess = () => {
    for (const queryKey of [
      getOrdersGetQueryKey(workspaceId, id),
      getOrdersListQueryKey(workspaceId),
      getOrdersEventsQueryKey(workspaceId, id),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const done = useDone()
  const ok = (message: string) => () => {
    done.show(message)
    onSuccess()
  }
  const vars = { workspace: workspaceId, order: id }
  // Complete and cancel ask first; the button that opens the question sits in the Next step card.
  const [ask, setAsk] = React.useState<"complete" | "cancel" | null>(null)
  const confirmKey = useIdempotencyKey()
  const completeKey = useIdempotencyKey()
  const cancelKey = useIdempotencyKey()
  const invoiceKey = useIdempotencyKey()
  const confirm = useOrdersConfirm({
    mutation: {
      onMutate: done.clear,
      onSuccess: ok(t("done.order.confirmed")),
      onSettled: (_d, e) => confirmKey.settle(e),
    },
    request: confirmKey.request,
  })
  const complete = useOrdersComplete({
    mutation: {
      onMutate: done.clear,
      onSuccess: ok(t("done.order.completed")),
      onSettled: (_d, e) => completeKey.settle(e),
    },
    request: completeKey.request,
  })
  const cancel = useOrdersCancel({
    mutation: {
      onMutate: done.clear,
      onSuccess: ok(t("done.order.cancelled")),
      onSettled: (_d, e) => cancelKey.settle(e),
    },
    request: cancelKey.request,
  })
  const invoice = useOrdersInvoice({
    mutation: {
      onMutate: done.clear,
      onSuccess: ok(t("done.order.invoiced")),
      onSettled: (_d, e) => invoiceKey.settle(e),
    },
    request: invoiceKey.request,
  })
  const actionError =
    confirm.error ?? complete.error ?? cancel.error ?? invoice.error

  if (order.isPending) return <DetailSkeleton />
  if (order.isError)
    return (
      <LoadError
        error={order.error}
        onRetry={() => void order.refetch()}
        backHref="/orders"
      />
    )
  const o = order.data.data
  const money = (minor: number) =>
    formatMoney(minor, o.decimals, o.currency, locale)
  const when = (iso: string) =>
    new Intl.DateTimeFormat(locale, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(iso))
  const statusLabel = (s: string) =>
    t.has(`orders.statuses.${s}`) ? t(`orders.statuses.${s}`) : s
  const source = t.has(`orders.sources.${o.source}`)
    ? t(`orders.sources.${o.source}`)
    : o.source

  // The API lists what this order's status and payments allow; anything else would be a 409.
  const allowed = actionOrder.filter((a) => o.allowed_actions.includes(a))
  // Refund and cancel undo things, so they never get the big button.
  const main = allowed.find((a) => a !== "refund" && a !== "cancel")
  const rest = allowed.filter((a) => a !== main)
  const actionButton = (a: OrderAction, big: boolean) => {
    const look = big
      ? ({ size: "lg", className: "w-full" } as const)
      : ({ variant: "outline", className: "w-full" } as const)
    switch (a) {
      case "confirm":
        return (
          <Button
            key={a}
            {...look}
            disabled={confirm.isPending}
            onClick={() => confirm.mutate(vars)}
          >
            {t("orders.confirm")}
          </Button>
        )
      case "pick":
        return (
          <PickDialog
            key={a}
            order={o}
            button={<Button {...look} />}
            onDone={ok(t("done.order.picked"))}
          />
        )
      case "complete":
        return (
          <Button
            key={a}
            {...look}
            disabled={complete.isPending}
            onClick={() => setAsk("complete")}
          >
            {t("orders.complete")}
          </Button>
        )
      case "invoice":
        return (
          <Button
            key={a}
            {...look}
            disabled={invoice.isPending}
            onClick={() => invoice.mutate(vars)}
          >
            {t("orders.makeInvoice")}
          </Button>
        )
      case "return":
        return (
          <ReturnsDialog
            key={a}
            order={o}
            button={<Button {...look} />}
            onDone={ok(t("done.order.returned"))}
          />
        )
      case "refund":
        return (
          <RefundDialog
            key={a}
            order={o}
            button={<Button {...look} />}
            onDone={onSuccess}
          />
        )
      case "cancel":
        return (
          <Button
            key={a}
            {...look}
            className="w-full text-(--tone-error)"
            disabled={cancel.isPending}
            onClick={() => setAsk("cancel")}
          >
            {t("orders.cancel")}
          </Button>
        )
    }
  }

  const c = (o.customer ?? {}) as {
    email?: string | null
    shipping_address?: Record<string, unknown> | null
  }
  const name = customerName(o.customer)
  const address = Object.values(c.shipping_address ?? {}).filter(
    (v): v is string => typeof v === "string" && v.trim() !== ""
  )
  const kind = (k: string) =>
    t.has(`orders.eventKinds.${k}`) ? t(`orders.eventKinds.${k}`) : k
  const recorded = trail.data?.data ?? []
  // The trail (newest first) read oldest first. An older order has none: its own timestamps
  // then stand in, without who did what.
  const events: {
    key: string
    kind: string
    at: string
    who?: string | null
  }[] = recorded.length
    ? recorded
        .map((e) => ({ key: e.id, kind: e.kind, at: e.at, who: e.actor?.name }))
        .reverse()
    : (
        [
          ["created", o.created_at],
          ["paid", o.paid_at],
          ["confirmed", o.confirmed_at],
          ["picked", o.picked_at],
          ["completed", o.completed_at],
          ["cancelled", o.cancelled_at],
        ] as const
      )
        .flatMap(([k, at]) => (at ? [{ key: k, kind: k, at }] : []))
        .sort((a, b) => Date.parse(a.at) - Date.parse(b.at))

  return (
    <>
      <PageHead
        back={{ href: "/orders", label: t("orders.title") }}
        title={t("orders.detailTitle", { number: o.number })}
        description={t("orders.placed", { source, date: when(o.created_at) })}
        actions={
          <StatusBadge status={o.status} label={statusLabel(o.status)} />
        }
      />
      <Bento>
        <Tile
          id="order-lines"
          title={t("invoices.lines")}
          action={
            <span className="text-[13px] text-muted-foreground">
              {/* `units` is the API's own sum of the quantities; only formatted here. */}
              {t("orders.itemsUnits", {
                count: o.lines.length,
                units: Number(o.units),
              })}
            </span>
          }
          span="col-span-8 row-span-2 max-md:row-span-1"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("invoices.item")}</TableHead>
                <TableHead className="text-right">
                  {t("invoices.quantity")}
                </TableHead>
                <TableHead className="text-right">
                  {t("orders.picked")}
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
                  <TableCell>
                    <span className="flex items-center gap-3">
                      <span
                        aria-hidden="true"
                        className="inline-flex size-10 shrink-0 items-center justify-center overflow-hidden rounded-[10px] border border-(--hair) bg-(--glass-subtle) text-muted-foreground max-md:hidden"
                      >
                        {l.image ? (
                          <img
                            src={l.image.url}
                            alt=""
                            referrerPolicy="no-referrer"
                            className="size-full object-cover"
                          />
                        ) : (
                          <Package className="size-[18px]" strokeWidth={1.75} />
                        )}
                      </span>
                      <span className="min-w-0">
                        <span className="block font-semibold">{l.title}</span>
                        {l.lots.length ? (
                          <span className="block font-mono text-xs text-muted-foreground">
                            {l.lots
                              .map((lot) => t("orders.lot", { code: lot.code }))
                              .join(" · ")}
                          </span>
                        ) : null}
                        {Number(l.returned) ? (
                          <span className="block text-xs text-muted-foreground">
                            {t("orders.returned")} {l.returned}
                          </span>
                        ) : null}
                      </span>
                    </span>
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {l.quantity}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {l.picked}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {money(l.unit_price)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {money(l.line_excl)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {/* The server's totals in minor units: formatted, never summed here. */}
          <div className="mt-auto flex flex-col gap-1 border-t border-(--hair) px-5 py-4">
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">
                {t("orders.subtotal")}
              </span>
              <span className="tabular-nums">{money(o.subtotal)}</span>
            </p>
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">{t("orders.vat")}</span>
              <span className="tabular-nums">{money(o.vat)}</span>
            </p>
            <p className="flex items-center justify-between">
              <span className="font-semibold">{t("invoices.total")}</span>
              <span className="text-lg font-semibold tabular-nums max-md:text-base">
                {money(o.total)}
              </span>
            </p>
            <p className="text-xs text-muted-foreground">
              {t("orders.serverAmounts")}
            </p>
          </div>
        </Tile>

        <SideCard id="order-next" title={t("orders.nextStep")}>
          <Done message={done.message} />
          {allowed.length === 0 ? (
            <Notice title={t("orders.nothingToDo")} />
          ) : null}
          {main ? (
            <>
              <p className="text-[13px]">{t(`orders.next.${main}`)}</p>
              {actionButton(main, true)}
            </>
          ) : null}
          {rest.length ? (
            <div className="grid grid-cols-2 gap-2 max-md:grid-cols-1">
              {rest.map((a) => actionButton(a, false))}
            </div>
          ) : null}
          <ApiError error={actionError} fallback={t("common.error")} />
        </SideCard>

        <SideCard id="order-customer" title={t("orders.customer")}>
          {name ? (
            <div className="flex items-center gap-3">
              <span
                aria-hidden="true"
                className="inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-[image:var(--gold-gradient)] text-[13px] font-semibold text-[#0a183a]"
              >
                {initials(name)}
              </span>
              <div className="min-w-0">
                <p className="font-semibold break-words">{name}</p>
                {c.email && c.email !== name ? (
                  <p className="text-xs break-words text-muted-foreground">
                    {c.email}
                  </p>
                ) : null}
              </div>
            </div>
          ) : (
            <p className="text-[13px] text-muted-foreground">
              {t("orders.noCustomer")}
            </p>
          )}
          {address.length ? (
            <SubTile className="text-[13px]">
              {address.map((line, i) => (
                <p key={i}>{line}</p>
              ))}
            </SubTile>
          ) : null}
          <p className="flex items-center justify-between gap-3 text-[13px]">
            <span className="text-muted-foreground">{t("orders.payment")}</span>
            {o.paid_at ? (
              <Badge variant="teal">
                {o.payment_method
                  ? t("orders.paidVia", {
                      method: t.has(`orders.methods.${o.payment_method}`)
                        ? t(`orders.methods.${o.payment_method}`)
                        : o.payment_method,
                    })
                  : t("orders.paid")}
              </Badge>
            ) : o.invoice_status === "paid" ||
              o.invoice_status === "partially_paid" ? (
              // Office orders are paid through their invoice; the API reads its status.
              <Badge variant={o.invoice_status === "paid" ? "teal" : "gold"}>
                {t(`orders.byInvoice.${o.invoice_status}`)}
              </Badge>
            ) : (
              <Badge variant="slate">{t("orders.notPaid")}</Badge>
            )}
          </p>
          {o.reference ? (
            <p className="flex items-center justify-between gap-3 text-[13px]">
              <span className="text-muted-foreground">
                {t("orders.reference")}
              </span>
              <span className="break-words">{o.reference}</span>
            </p>
          ) : null}
          {o.invoice_id ? (
            <p className="flex items-center justify-between gap-3 text-[13px]">
              <span className="text-muted-foreground">
                {t("orders.invoice")}
              </span>
              <Link
                className="inline-flex items-center font-medium underline-offset-4 hover:underline max-md:min-h-11"
                href={`/invoices/${o.invoice_id}`}
              >
                {t("orders.openInvoice")}
              </Link>
              {o.invoice_status ? (
                <StatusBadge
                  status={o.invoice_status}
                  label={t(`invoices.statuses.${o.invoice_status}`)}
                />
              ) : null}
            </p>
          ) : null}
        </SideCard>

        <Tile
          id="order-timeline"
          title={t("orders.timeline")}
          span="col-span-12"
        >
          {trail.isPending ? (
            <Skeleton className="h-[22px] w-2/3" />
          ) : trail.isError ? (
            <LoadError
              error={trail.error}
              onRetry={() => void trail.refetch()}
            />
          ) : (
            <>
              <ol className="flex flex-wrap items-center gap-x-6 gap-y-3 text-[13px]">
                {events.map((e, i) => (
                  <li key={e.key} className="flex items-center gap-6">
                    {i > 0 ? (
                      <ChevronRight
                        aria-hidden="true"
                        className="size-4 text-muted-foreground"
                        strokeWidth={1.75}
                      />
                    ) : null}
                    <span className="flex items-center gap-2">
                      <StatusBadge
                        status={e.kind}
                        label={kind(e.kind)}
                        tone={kindTone[e.kind]}
                      />
                      <span className="text-muted-foreground">
                        {when(e.at)}
                        {e.who ? ` · ${e.who}` : ""}
                      </span>
                    </span>
                  </li>
                ))}
              </ol>
              {recorded.length ? null : (
                <p className="text-xs text-muted-foreground">
                  {t("orders.trailNote")}
                </p>
              )}
            </>
          )}
        </Tile>
      </Bento>

      <ConfirmDialog
        open={ask === "complete"}
        onOpenChange={(open) => setAsk(open ? "complete" : null)}
        destructive={false}
        title={t("orders.completeTitle", { number: o.number })}
        description={t("orders.completeText")}
        confirm={t("orders.complete")}
        cancel={t("actions.cancel")}
        disabled={complete.isPending}
        onConfirm={() => complete.mutate(vars)}
      />
      <ConfirmDialog
        open={ask === "cancel"}
        onOpenChange={(open) => setAsk(open ? "cancel" : null)}
        title={t("orders.cancelTitle", { number: o.number })}
        description={t("orders.cancelText")}
        confirm={t("orders.cancel")}
        cancel={t("actions.cancel")}
        disabled={cancel.isPending}
        onConfirm={() => cancel.mutate(vars)}
      />
    </>
  )
}

/** A card with the small caps title of OfficeOrder's "Next step" and "Customer". */
function SideCard({
  id,
  title,
  children,
}: {
  id: string
  title: string
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className="col-span-4 gap-4 px-6 py-6 max-md:col-span-12 max-md:gap-3 max-md:p-4"
    >
      <h2
        id={id}
        className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
      >
        {title}
      </h2>
      {children}
    </Card>
  )
}

/** Per-line quantities typed by staff; a dialog's Cancel sends nothing. */
function LinesDialog({
  order,
  trigger,
  button,
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
  /** The Button that opens it, sized by where it sits. */
  button: React.ReactElement
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
      <DialogTrigger render={button}>{trigger}</DialogTrigger>
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
  button,
  onDone,
}: {
  order: OrderResource
  button: React.ReactElement
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
      button={button}
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
  button,
  onDone,
}: {
  order: OrderResource
  button: React.ReactElement
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
      button={button}
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
  button,
  onDone,
}: {
  order: OrderResource
  button: React.ReactElement
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
      <DialogTrigger render={button}>{t("orders.refund")}</DialogTrigger>
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
