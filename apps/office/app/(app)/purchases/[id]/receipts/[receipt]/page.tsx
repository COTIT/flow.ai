"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  usePurchaseOrdersGet,
} from "@genesy/api-client/endpoints/purchase-orders"
import {
  usePurchaseReceiptsGet,
} from "@genesy/api-client/endpoints/purchase-receipts"
import { useWarehousesList } from "@genesy/api-client/endpoints/warehouses"
import type { CurrencyRef } from "@genesy/api-client/model"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Notice } from "@genesy/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDateTime, formatDay, formatQty } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

import { bookAmount, showMoney, showRate, useRefreshOrder, useWord } from "../../../shared"
import type { Outcome } from "../../dialogs"
import { BookDialog } from "./book-dialog"

/** One receipt (Detail tiles): its lines and their state, values once booked, extra costs, Book. */
export default function ReceiptPage({
  params,
}: {
  params: Promise<{ id: string; receipt: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = useWord()
  const { id, receipt: receiptId } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const order = usePurchaseOrdersGet(workspaceId, id, enabled)
  const receipt = usePurchaseReceiptsGet(workspaceId, id, receiptId, enabled)
  const warehouses = useWarehousesList(workspaceId, { page_size: 100 }, enabled)
  const refresh = useRefreshOrder(workspaceId, id)
  const mayBook = useCan("purchaseorder.book")
  const mayReturn = useCan("purchasereturn.create")
  const done = useDone()
  const [stale, setStale] = React.useState(false)
  const outcome: Outcome = {
    done: (message) => {
      setStale(false)
      done.show(message)
      refresh(receiptId)
    },
    stale: () => {
      done.clear()
      setStale(true)
      refresh(receiptId)
    },
  }

  if (order.isPending || receipt.isPending) return <DetailSkeleton />
  const failed = order.isError ? order : receipt.isError ? receipt : null
  if (failed)
    return (
      <LoadError
        error={failed.error}
        onRetry={() => void failed.refetch()}
        backHref={`/purchases/${id}`}
      />
    )
  const o = order.data!.data
  const r = receipt.data!.data
  // Receipt lines carry ids only; names come from the order's lines and the warehouse list.
  const lineOf = (poLine: string) => o.lines.find((l) => l.id === poLine)
  const name = (receiptLine: string) => {
    const rl = r.lines.find((l) => l.id === receiptLine)
    return (rl && lineOf(rl.po_line_id)?.product.name) ?? ""
  }
  const warehouse = (wid: string) => warehouses.data?.data.find((w) => w.id === wid)?.name ?? ""
  // The book's own currency (null in a setup book, where nothing is valued).
  // shortcut: cast until the regenerated client carries functional_currency.
  const book = (r as { functional_currency?: CurrencyRef | null }).functional_currency
  const value = (amount: string | null | undefined) =>
    amount == null ? "" : book ? showMoney(amount, book, locale) : bookAmount(amount, locale)
  const canBook = r.allowed_actions.includes("book") && mayBook
  // Booked goods may go back to the supplier; the return page reads what is still returnable.
  const canReturn = mayReturn && r.lines.some((l) => l.state === "booked")

  return (
    <>
      <PageHead
        back={{ href: `/purchases/${o.id}`, label: t("purchases.detailTitle", { number: o.number }) }}
        title={t("purchases.receiptTitle", { number: r.number })}
        description={t("purchases.receiptOf", { number: o.number })}
        actions={
          <StatusBadge
            status={r.booking_status}
            label={word("bookingStatus", r.booking_status)}
            tone={r.booking_status === "booked" ? "teal" : "gold"}
          />
        }
      />
      {stale ? <Notice variant="warning" title={t("purchases.stale")} /> : null}
      <Bento>
        <Tile
          id="rc-lines"
          title={t("purchases.lines")}
          span="col-span-8 row-span-2 max-md:row-span-1"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("purchases.product")}</TableHead>
                <TableHead className="text-right">{t("purchases.quantity")}</TableHead>
                <TableHead>{t("purchases.state")}</TableHead>
                <TableHead className="text-right">{t("purchases.value")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {r.lines.map((l) => (
                <TableRow key={l.id}>
                  <TableCell>
                    <span className="block font-semibold">{name(l.id)}</span>
                    <span className="block text-xs text-muted-foreground">
                      {warehouse(l.warehouse_id)}
                      {l.rate && l.rate.source !== "identity"
                        ? ` · ${t("purchases.rate", { rate: showRate(l.rate.rate, locale) })}`
                        : ""}
                    </span>
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {formatQty(l.base_quantity, locale)}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={l.state}
                      label={word("lineState", l.state)}
                      tone={l.state === "booked" ? "teal" : l.state === "late" ? "red" : "gold"}
                    />
                  </TableCell>
                  {/* Values are in the book's own currency, as the API stored them. */}
                  <TableCell data-m="right" className="text-right font-semibold tabular-nums">
                    {value(l.value)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Tile>

        <SideCard id="rc-next" title={t("purchases.nextStep")}>
          <Done message={done.message} />
          {r.lines.some((l) => l.state === "late") ? (
            <Notice variant="warning" title={t("purchases.lateWarning")} />
          ) : null}
          {canBook ? (
            <>
              <p className="text-[13px]">{t("purchases.next.book")}</p>
              <BookDialog
                order={o}
                receipt={r}
                name={name}
                button={<Button size="lg" className="w-full" />}
                outcome={outcome}
              />
            </>
          ) : canReturn ? null : (
            <Notice title={t("purchases.nothingToDo")} />
          )}
          {canReturn ? (
            <Link
              href={`/purchase-returns/new?receipt=${r.id}`}
              className={buttonVariants({ variant: "outline", className: "w-full" })}
            >
              {t("payables.returns.returnGoods")}
            </Link>
          ) : null}
        </SideCard>

        <SideCard id="rc-facts" title={t("purchases.facts")}>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact label={t("purchases.effectiveDate")} value={formatDay(r.effective_receipt_date, locale)} />
            <Fact label={t("purchases.recordedAt")} value={formatDateTime(r.recorded_at, locale)} />
            <Fact label={t("purchases.supplier")} value={o.supplier.name ?? ""} />
          </SubTile>
          {r.note ? <p className="text-[13px] break-words">{r.note}</p> : null}
        </SideCard>

        {r.landed_costs.length ? (
          <Tile id="rc-costs" title={t("purchases.landedCosts")} span="col-span-12" flush>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("purchases.evidenceRef")}</TableHead>
                  <TableHead className="text-right">{t("purchases.amount")}</TableHead>
                  <TableHead>{t("purchases.allocations")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {r.landed_costs.map((c) => (
                  <TableRow key={c.id}>
                    <TableCell>{c.evidence_ref}</TableCell>
                    <TableCell data-m="right" className="text-right tabular-nums">
                      {showMoney(c.amount, c.currency, locale)}
                    </TableCell>
                    <TableCell data-m="sub" className="text-[13px]">
                      {c.allocations
                        .map((a) => `${name(a.receipt_line_id)} ${value(a.functional_amount)}`)
                        .join(" · ")}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </Tile>
        ) : null}
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
