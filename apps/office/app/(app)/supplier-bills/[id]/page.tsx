"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  getPurchaseCostAdjustmentsListQueryKey,
  usePurchaseCostAdjustmentsList,
} from "@verzo/api-client/endpoints/purchase-cost-adjustments"
import {
  getSupplierBillsGetQueryKey,
  getSupplierBillsListQueryKey,
  useSupplierBillsGet,
} from "@verzo/api-client/endpoints/supplier-bills"
import { Button, buttonVariants } from "@verzo/ui/components/button"
import { Notice } from "@verzo/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { formatDay, formatQty } from "@verzo/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { PageHead } from "@/components/list"
import { Fact, SideCard, usePayWord } from "@/components/payables"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import { bookAmount, showMoney, showRate } from "../../purchases/shared"
import type { Outcome } from "../../purchases/[id]/dialogs"
import {
  CostActionDialog,
  CostAdjustDialog,
  MatchDialog,
  PostBillDialog,
  ReverseBillDialog,
} from "./dialogs"

const actionOrder = ["match", "post", "update", "reverse"] as const

/** A supplier bill (Detail tiles from the purchase order): lines + matches, server totals, next step. */
export default function SupplierBillPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = usePayWord()
  const { id } = React.use(params)
  const { done: arrived } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const rights = useRights()
  const bill = useSupplierBillsGet(workspaceId, id, { query: { enabled: Boolean(workspaceId) } })
  const mayCosts = rights.may("accounting.read")
  const costs = usePurchaseCostAdjustmentsList(
    workspaceId,
    { bill_id: id },
    { query: { enabled: Boolean(workspaceId) && mayCosts && bill.data?.data.state !== "draft" } }
  )
  const done = useDone(
    arrived === "created" || arrived === "saved" ? t(`payables.done.${arrived}`) : null
  )
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getSupplierBillsGetQueryKey(workspaceId, id),
      getSupplierBillsListQueryKey(workspaceId),
      getPurchaseCostAdjustmentsListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
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

  if (bill.isPending) return <DetailSkeleton />
  if (bill.isError)
    return <LoadError error={bill.error} onRetry={() => void bill.refetch()} backHref="/supplier-bills" />
  const b = bill.data.data
  const money = (amount: string) => showMoney(amount, b.currency, locale)
  const allowed = actionOrder.filter(
    (a) => b.allowed_actions.includes(a) && rights.may(`supplierbill.${a}`)
  )
  const actionButton = (a: (typeof actionOrder)[number], big: boolean) => {
    const button = big ? <Button size="lg" className="w-full" /> : <Button variant="outline" className="w-full" />
    switch (a) {
      case "match":
        return <MatchDialog key={a} bill={b} button={button} outcome={outcome} />
      case "post":
        return <PostBillDialog key={a} bill={b} button={button} outcome={outcome} />
      case "reverse":
        return <ReverseBillDialog key={a} bill={b} button={button} outcome={outcome} />
      case "update":
        return (
          <Link
            key={a}
            href={`/supplier-bills/${b.id}/edit`}
            className={buttonVariants({ variant: "outline", className: "w-full" })}
          >
            {t("payables.bills.edit")}
          </Link>
        )
    }
  }
  const [main, ...rest] = allowed
  const adjs = costs.data?.data ?? []

  return (
    <>
      <PageHead
        back={{ href: "/supplier-bills", label: t("payables.bills.title") }}
        title={t("payables.bills.detailTitle", { number: b.number })}
        description={`${b.supplier.name ?? ""} · ${b.supplier_invoice_number}`}
        actions={<StatusBadge status={b.state} label={word("states", b.state)} />}
      />
      {stale ? <Notice variant="warning" title={t("payables.stale")} /> : null}
      <Bento>
        <Tile id="bill-lines" title={t("payables.lines")} span="col-span-8 row-span-2 max-md:row-span-1" flush>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("payables.item")}</TableHead>
                <TableHead className="text-right">{t("payables.quantity")}</TableHead>
                <TableHead>{t("payables.matches")}</TableHead>
                <TableHead className="text-right">{t("payables.extension")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {b.lines.map((l) => (
                <TableRow key={l.id}>
                  <TableCell>
                    <span className="block font-semibold">
                      {l.product_name ?? l.description ?? word("kinds", l.kind)}
                    </span>
                    <span className="block text-xs text-muted-foreground">
                      {word("kinds", l.kind)} · {money(l.unit_price)}
                    </span>
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {formatQty(l.quantity, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-[13px]">
                    {l.kind !== "goods" ? null : l.matches.length === 0 ? (
                      <span className="text-muted-foreground">{t("payables.bills.unmatched")}</span>
                    ) : (
                      <ul className="flex flex-col gap-0.5">
                        {l.matches.map((m) => (
                          <li key={m.id}>
                            {formatQty(m.quantity, locale)}{" "}
                            {m.receipt_id && m.purchase_order_id ? (
                              <Link
                                className="underline-offset-4 hover:underline"
                                href={`/purchases/${m.purchase_order_id}/receipts/${m.receipt_id}`}
                              >
                                {t("payables.bills.receiptLink")}
                              </Link>
                            ) : null}{" "}
                            <span className="text-muted-foreground">({word("matchStates", m.state)})</span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </TableCell>
                  <TableCell data-m="right" className="text-right font-semibold tabular-nums">
                    {money(l.extension)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {/* The server's totals as strings: formatted, never summed here. */}
          <div className="mt-auto flex flex-col gap-1 border-t border-(--hair) px-5 py-4 text-[13px]">
            <Fact label={t("payables.net")} value={money(b.totals.net)} />
            {b.totals.tax ? <Fact label={t("payables.tax")} value={money(b.totals.tax)} /> : null}
            {b.totals.total ? (
              <p className="flex items-center justify-between">
                <span className="font-semibold">{t("payables.total")}</span>
                <span className="text-lg font-semibold tabular-nums max-md:text-base">{money(b.totals.total)}</span>
              </p>
            ) : (
              <p className="text-xs text-muted-foreground">{t("payables.bills.totalAtPost")}</p>
            )}
            {b.totals.functional_total ? (
              <Fact label={t("payables.functionalTotal")} value={bookAmount(b.totals.functional_total, locale)} />
            ) : null}
          </div>
        </Tile>

        <SideCard id="bill-next" title={t("payables.nextStep")}>
          <Done message={done.message} />
          {b.state === "reversed" ? (
            <Notice
              title={t("payables.reversedInfo", {
                date: b.reversal_date ? formatDay(b.reversal_date, locale) : "",
                reason: b.reverse_reason ?? "",
              })}
            />
          ) : allowed.length === 0 ? (
            <Notice title={t("payables.nothingToDo")} />
          ) : null}
          {main ? (
            <>
              <p className="text-[13px]">{t(`payables.bills.next.${main}`)}</p>
              {actionButton(main, true)}
            </>
          ) : null}
          {rest.length ? (
            <div className="grid grid-cols-2 gap-2 max-md:grid-cols-1">{rest.map((a) => actionButton(a, false))}</div>
          ) : null}
        </SideCard>

        <SideCard id="bill-facts" title={t("payables.facts")}>
          <p className="font-semibold break-words">{b.supplier.name ?? ""}</p>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact label={t("payables.invoiceNumber")} value={b.supplier_invoice_number} />
            <Fact label={t("payables.invoiceDate")} value={formatDay(b.invoice_date, locale)} />
            {b.due_date ? <Fact label={t("payables.dueDate")} value={formatDay(b.due_date, locale)} /> : null}
            <Fact label={t("payables.accountingDate")} value={formatDay(b.accounting_date, locale)} />
            <Fact label={t("payables.currency")} value={b.currency.code} />
            {b.rate ? <Fact label={t("payables.rate")} value={showRate(b.rate.rate, locale)} /> : null}
            {b.evidence_ref ? <Fact label={t("payables.evidenceRef")} value={b.evidence_ref} /> : null}
            {b.corrects_bill_id ? (
              <Fact
                label={t("payables.bills.corrects")}
                value={
                  <Link className="underline-offset-4 hover:underline" href={`/supplier-bills/${b.corrects_bill_id}`}>
                    {t("payables.bills.original")}
                  </Link>
                }
              />
            ) : null}
          </SubTile>
          {b.note ? <p className="text-[13px] break-words">{b.note}</p> : null}
        </SideCard>

        {b.state !== "draft" && mayCosts ? (
          <Tile id="bill-costs" title={t("payables.costs.title")} span="col-span-12">
            <div className="flex flex-col gap-3 text-[13px]">
              <p className="text-muted-foreground">{t("payables.costs.hint")}</p>
              {costs.isError ? (
                <LoadError error={costs.error} onRetry={() => void costs.refetch()} />
              ) : adjs.length === 0 ? (
                <p className="text-muted-foreground">{t("payables.costs.empty")}</p>
              ) : (
                <ul className="flex flex-col gap-2">
                  {adjs.map((c) => (
                    <li key={c.id} className="flex flex-wrap items-center justify-between gap-2 border-t border-(--hair) pt-2">
                      <span className="flex flex-wrap items-center gap-2">
                        <StatusBadge status={c.state} label={word("states", c.state)} />
                        <span>{c.reason}</span>
                        <span className="text-muted-foreground">
                          {t("payables.costs.pendingLeft", {
                            amount: showMoney(c.bill_pending_variance, c.functional_currency, locale),
                          })}
                        </span>
                      </span>
                      <span className="flex gap-2">
                        {c.allowed_actions.includes("post") && rights.may("accounting.post") ? (
                          <CostActionDialog adj={c} action="post" button={<Button variant="outline" />} outcome={outcome} />
                        ) : null}
                        {c.allowed_actions.includes("reverse") && rights.may("accounting.reverse") ? (
                          <CostActionDialog adj={c} action="reverse" button={<Button variant="outline" />} outcome={outcome} />
                        ) : null}
                      </span>
                    </li>
                  ))}
                </ul>
              )}
              {b.state === "posted" && rights.may("accounting.journal_create") ? (
                <CostAdjustDialog bill={b} button={<Button variant="outline" className="self-start" />} outcome={outcome} />
              ) : null}
            </div>
          </Tile>
        ) : null}
      </Bento>
    </>
  )
}
