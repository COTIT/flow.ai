"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { ChevronRight } from "lucide-react"

import { useSession } from "@verzo/api-client/auth"
import {
  usePurchaseOrdersEvents,
  usePurchaseOrdersGet,
} from "@verzo/api-client/endpoints/purchase-orders"
import {
  usePurchaseReceiptsList,
} from "@verzo/api-client/endpoints/purchase-receipts"
import { Button, buttonVariants } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { Notice } from "@verzo/ui/components/notice"
import { Skeleton } from "@verzo/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { formatDateTime, formatDay, formatQty } from "@verzo/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import { showMoney, useRefreshOrder, useWord } from "../shared"
import {
  CancelDialog,
  CloseDialog,
  ConfirmPoDialog,
  ReceiveDialog,
  type Outcome,
} from "./dialogs"

const actionOrder = ["confirm", "receive", "update", "close", "cancel"] as const
type Action = (typeof actionOrder)[number]

/** A purchase order (Detail tiles from OfficeOrder): lines + server totals, next step, receipts, history. */
export default function PurchaseOrderPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = useWord()
  const { id } = React.use(params)
  const { done: arrived } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const order = usePurchaseOrdersGet(workspaceId, id, enabled)
  const receipts = usePurchaseReceiptsList(workspaceId, id, undefined, enabled)
  const trail = usePurchaseOrdersEvents(workspaceId, id, enabled)
  const refresh = useRefreshOrder(workspaceId, id)
  const rights = useRights()
  const done = useDone(
    arrived === "created" || arrived === "saved" ? t(`done.purchase.${arrived}`) : null
  )
  const [stale, setStale] = React.useState(false)
  const outcome: Outcome = {
    done: (message) => {
      setStale(false)
      done.show(message)
      refresh()
    },
    stale: () => {
      done.clear()
      setStale(true)
      refresh()
    },
  }

  if (order.isPending) return <DetailSkeleton />
  if (order.isError)
    return (
      <LoadError
        error={order.error}
        onRetry={() => void order.refetch()}
        backHref="/purchases"
      />
    )
  const o = order.data.data
  const money = (amount: string) => showMoney(amount, o.currency, locale)
  // What the API allows on this order now, narrowed to what this member may do.
  const allowed = actionOrder.filter(
    (a) => o.allowed_actions.includes(a) && rights.may(`purchaseorder.${a}`)
  )
  // Close and cancel give up on goods, so they never get the big button.
  const main = allowed.find((a) => a === "confirm" || a === "receive")
  const rest = allowed.filter((a) => a !== main)
  const actionButton = (a: Action, big: boolean) => {
    const look = big
      ? ({ size: "lg", className: "w-full" } as const)
      : ({ variant: "outline", className: "w-full" } as const)
    const button = <Button {...look} />
    switch (a) {
      case "confirm":
        return <ConfirmPoDialog key={a} order={o} button={button} outcome={outcome} />
      case "receive":
        return <ReceiveDialog key={a} order={o} button={button} outcome={outcome} />
      case "update":
        return (
          <Link
            key={a}
            href={`/purchases/${o.id}/edit`}
            className={buttonVariants({ variant: "outline", className: "w-full" })}
          >
            {t("purchases.edit")}
          </Link>
        )
      case "close":
        return <CloseDialog key={a} order={o} button={button} outcome={outcome} />
      case "cancel":
        return <CancelDialog key={a} order={o} button={button} outcome={outcome} />
    }
  }
  const snapshot = o.supplier_snapshot as { company_name?: string | null } | null | undefined
  const events = [...(trail.data?.data ?? [])].reverse()

  return (
    <>
      <PageHead
        back={{ href: "/purchases", label: t("purchases.title") }}
        title={t("purchases.detailTitle", { number: o.number })}
        description={`${o.supplier.name ?? ""} · ${formatDay(o.purchase_date, locale)}`}
        actions={
          <span className="flex flex-wrap gap-1.5">
            <StatusBadge status={o.state} label={word("states", o.state)} />
            {o.state === "confirmed" ? (
              <StatusBadge
                status={o.receipt_status}
                label={word("receiptStatus", o.receipt_status)}
                tone={o.receipt_status === "received" ? "teal" : "gold"}
              />
            ) : null}
          </span>
        }
      />
      {stale ? <Notice variant="warning" title={t("purchases.stale")} /> : null}
      <Bento>
        <Tile
          id="po-lines"
          title={t("purchases.lines")}
          span="col-span-8 row-span-2 max-md:row-span-1"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("purchases.product")}</TableHead>
                <TableHead className="text-right">{t("purchases.ordered")}</TableHead>
                <TableHead className="text-right">{t("purchases.received")}</TableHead>
                <TableHead className="text-right">{t("purchases.booked")}</TableHead>
                <TableHead className="text-right">{t("purchases.open")}</TableHead>
                <TableHead className="text-right">{t("purchases.extension")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {o.lines.map((l) => (
                <TableRow key={l.id}>
                  <TableCell>
                    <span className="block font-semibold">{l.product.name}</span>
                    <span className="block text-xs text-muted-foreground">
                      {formatQty(l.input_quantity, locale)}
                      {l.purchase_unit ? ` ${l.purchase_unit.code}` : ""} × {money(l.unit_price)}
                      {Number(l.closed_quantity)
                        ? ` · ${t("purchases.closed")} ${formatQty(l.closed_quantity, locale)}`
                        : ""}
                    </span>
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {formatQty(l.base_quantity, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {formatQty(l.received_quantity, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {formatQty(l.booked_quantity, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {formatQty(l.open_quantity, locale)}
                  </TableCell>
                  <TableCell data-m="right" className="text-right font-semibold tabular-nums">
                    {money(l.extension)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {/* The server's totals as strings: formatted, never summed here. */}
          <div className="mt-auto flex flex-col gap-1 border-t border-(--hair) px-5 py-4">
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">{t("purchases.totalReceived")}</span>
              <span className="tabular-nums">{money(o.totals.received)}</span>
            </p>
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">{t("purchases.totalOpen")}</span>
              <span className="tabular-nums">{money(o.totals.open)}</span>
            </p>
            <p className="flex items-center justify-between">
              <span className="font-semibold">{t("purchases.totalOrdered")}</span>
              <span className="text-lg font-semibold tabular-nums max-md:text-base">
                {money(o.totals.ordered)}
              </span>
            </p>
            <p className="text-xs text-muted-foreground">{t("purchases.serverAmounts")}</p>
          </div>
        </Tile>

        <SideCard id="po-next" title={t("purchases.nextStep")}>
          <Done message={done.message} />
          {o.state === "cancelled" ? (
            <Notice
              title={t("purchases.cancelledInfo", { reason: o.cancel_reason ?? "" })}
            />
          ) : allowed.length === 0 ? (
            <Notice title={t("purchases.nothingToDo")} />
          ) : null}
          {main ? (
            <>
              <p className="text-[13px]">{t(`purchases.next.${main}`)}</p>
              {actionButton(main, true)}
            </>
          ) : allowed.includes("update") ? (
            <p className="text-[13px]">{t("purchases.next.update")}</p>
          ) : null}
          {rest.length ? (
            <div className="grid grid-cols-2 gap-2 max-md:grid-cols-1">
              {rest.map((a) => actionButton(a, false))}
            </div>
          ) : null}
        </SideCard>

        <SideCard id="po-supplier" title={t("purchases.supplier")}>
          <p className="font-semibold break-words">
            {snapshot?.company_name ?? o.supplier.name ?? ""}
          </p>
          {o.state === "draft" ? (
            <p className="text-[13px] text-muted-foreground">{t("purchases.draftSupplier")}</p>
          ) : null}
          <SubTile className="flex flex-col gap-1 text-[13px]">
            {o.reference ? (
              <Fact label={t("purchases.reference")} value={o.reference} />
            ) : null}
            {o.expected_date ? (
              <Fact label={t("purchases.expectedDate")} value={formatDay(o.expected_date, locale)} />
            ) : null}
            <Fact label={t("purchases.currency")} value={o.currency.code} />
          </SubTile>
          {o.note ? <p className="text-[13px] break-words">{o.note}</p> : null}
        </SideCard>

        <Tile id="po-receipts" title={t("purchases.receipts")} span="col-span-12" flush>
          {receipts.isPending ? (
            <div className="px-5 py-4">
              <Skeleton className="h-5 w-1/2" />
            </div>
          ) : receipts.isError ? (
            <div className="px-5 py-4">
              <LoadError error={receipts.error} onRetry={() => void receipts.refetch()} />
            </div>
          ) : receipts.data.data.length === 0 ? (
            <p className="px-5 py-4 text-[13px] text-muted-foreground">{t("purchases.noReceipts")}</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("purchases.receiptNumber")}</TableHead>
                  <TableHead>{t("purchases.effectiveDate")}</TableHead>
                  <TableHead>{t("purchases.state")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {receipts.data.data.map((r) => (
                  <TableRow key={r.id}>
                    <TableCell>
                      <Link
                        className="font-semibold underline-offset-4 hover:underline"
                        href={`/purchases/${o.id}/receipts/${r.id}`}
                      >
                        {t("purchases.receiptTitle", { number: r.number })}
                      </Link>
                    </TableCell>
                    <TableCell data-m="sub">{formatDay(r.effective_receipt_date, locale)}</TableCell>
                    <TableCell data-m="badge">
                      <StatusBadge
                        status={r.booking_status}
                        label={word("bookingStatus", r.booking_status)}
                        tone={r.booking_status === "booked" ? "teal" : "gold"}
                      />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </Tile>

        <Tile id="po-timeline" title={t("purchases.timeline")} span="col-span-12">
          {trail.isPending ? (
            <Skeleton className="h-[22px] w-2/3" />
          ) : trail.isError ? (
            <LoadError error={trail.error} onRetry={() => void trail.refetch()} />
          ) : (
            <ol className="flex flex-wrap items-center gap-x-6 gap-y-3 text-[13px]">
              {events.map((e, i) => (
                <li key={`${e.kind}-${e.ref_id}-${e.at}`} className="flex items-center gap-6">
                  {i > 0 ? (
                    <ChevronRight aria-hidden="true" className="size-4 text-muted-foreground" strokeWidth={1.75} />
                  ) : null}
                  <span className="flex items-center gap-2">
                    <StatusBadge
                      status={e.kind}
                      label={word("eventKinds", e.kind)}
                      tone={e.kind === "cancelled" || e.kind === "closed" ? "gold" : "slate"}
                    />
                    <span className="text-muted-foreground">
                      {/* shortcut: actor is a user id; no name until the API sends one. */}
                      {formatDateTime(e.at, locale)}
                    </span>
                  </span>
                </li>
              ))}
            </ol>
          )}
        </Tile>
      </Bento>
    </>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <p className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="break-words">{value}</span>
    </p>
  )
}

/** OfficeOrder's small-caps side card ("Next step", "Supplier"). */
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
      <h2 id={id} className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
        {title}
      </h2>
      {children}
    </Card>
  )
}
