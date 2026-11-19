"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getSalesReturnsGetQueryKey,
  getSalesReturnsListQueryKey,
  useSalesReturnsGet,
} from "@genesy/api-client/endpoints/sales-returns"
import { Button } from "@genesy/ui/components/button"
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
import { useRights } from "@/lib/can"

import { bookAmount, useStateBadge, useWord, type Outcome } from "../shared"
import {
  CancelDialog,
  EditDialog,
  LegacyCostDialog,
  PostDialog,
  RestockDialog,
  RestockReverseDialog,
  ReverseDialog,
  useLineLabel,
} from "./dialogs"

const link =
  "underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"

/** One customer return (Detail tiles, as /stock/counts/[id]): lines, restocks, next step, facts. */
export default function SalesReturnPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const rights = useRights()
  const badge = useStateBadge()
  const word = useWord()
  const label = useLineLabel()
  const queryClient = useQueryClient()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const ret = useSalesReturnsGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  const done = useDone()
  const [stale, setStale] = React.useState(false)
  // Reviewed legacy costs recorded on this page, per return line, for the next restock.
  const [legacy, setLegacy] = React.useState<Record<string, string>>({})
  const refresh = () => {
    for (const queryKey of [
      getSalesReturnsGetQueryKey(workspaceId, id),
      getSalesReturnsListQueryKey(workspaceId),
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

  if (ret.isPending) return <DetailSkeleton />
  if (ret.isError)
    return (
      <LoadError
        error={ret.error}
        onRetry={() => void ret.refetch()}
        backHref="/sales-returns"
      />
    )
  const r = ret.data.data
  const may = (action: string, right: string) =>
    r.allowed_actions.includes(action) && rights.may(right)
  const mayUpdate = may("update", "salesreturn.update")
  const mayPost = may("post", "salesreturn.post")
  const mayCancel = may("cancel", "salesreturn.cancel")
  const mayRestock = may("restock", "salesreturn.post")
  const mayReverse = may("reverse", "salesreturn.reverse")
  const mayUnrestock = rights.may("salesreturn.reverse")
  const mayLegacy =
    mayRestock && Boolean(r.order_id) && rights.may("accounting.post")
  const title = r.order_number
    ? t("salesReturns.ofOrder", { number: r.order_number })
    : word("kinds", r.kind)

  return (
    <>
      <PageHead
        back={{ href: "/sales-returns", label: t("salesReturns.title") }}
        title={title}
        description={r.reason}
        actions={<StatusBadge status={r.state} {...badge(r.state)} />}
      />
      {stale ? (
        <Notice
          variant="warning"
          title={t("accounting.reasons.stale_document")}
        />
      ) : null}
      <Bento>
        <Tile
          id="sr-lines"
          title={t("salesReturns.lines")}
          span="col-span-8 max-md:row-span-1"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("salesReturns.item")}</TableHead>
                <TableHead className="text-right">
                  {t("salesReturns.quantity")}
                </TableHead>
                <TableHead className="text-right">
                  {t("salesReturns.restocked")}
                </TableHead>
                <TableHead className="text-right">
                  {t("salesReturns.net")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {r.lines.map((l) => (
                <TableRow key={l.id}>
                  <TableCell className="font-medium">
                    {label(l)}
                    {mayLegacy && l.order_line_id ? (
                      <span className="mt-1 block">
                        <LegacyCostDialog
                          ret={r}
                          line={l}
                          outcome={outcome}
                          onRecorded={(cost) =>
                            setLegacy((all) => ({ ...all, [l.id]: cost }))
                          }
                          button={
                            <Button
                              variant="link"
                              size="sm"
                              className="h-auto p-0 max-md:min-h-11"
                            />
                          }
                        />
                      </span>
                    ) : null}
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {formatQty(l.quantity, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {formatQty(l.restocked, locale)}
                  </TableCell>
                  <TableCell data-m="right" className="text-right tabular-nums">
                    {l.net != null ? bookAmount(l.net, locale) : "–"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </Tile>

        <SideCard id="sr-next" title={t("salesReturns.nextStep")}>
          <Done message={done.message} />
          {r.state === "posted" && r.credit != null ? (
            <Notice
              title={t("salesReturns.creditInfo", {
                credit: bookAmount(r.credit, locale),
              })}
            />
          ) : null}
          {mayPost ? (
            <>
              <p className="text-[13px]">{t("salesReturns.next.post")}</p>
              <PostDialog
                ret={r}
                outcome={outcome}
                button={<Button size="lg" className="w-full" />}
              />
            </>
          ) : null}
          {mayRestock ? (
            <>
              <p className="text-[13px]">{t("salesReturns.next.restock")}</p>
              <RestockDialog
                ret={r}
                outcome={outcome}
                legacy={legacy}
                button={<Button size="lg" className="w-full" />}
              />
            </>
          ) : null}
          {mayUpdate ? (
            <EditDialog
              ret={r}
              outcome={outcome}
              button={<Button variant="outline" className="w-full" />}
            />
          ) : null}
          {mayReverse ? (
            <ReverseDialog
              ret={r}
              outcome={outcome}
              button={<Button variant="outline" className="w-full" />}
            />
          ) : null}
          {mayCancel ? (
            <CancelDialog
              ret={r}
              outcome={outcome}
              button={<Button variant="outline" className="w-full" />}
            />
          ) : null}
          {!mayPost &&
          !mayRestock &&
          !mayUpdate &&
          !mayReverse &&
          !mayCancel ? (
            <Notice
              title={
                r.state === "reversed"
                  ? t("salesReturns.reversedInfo")
                  : r.state === "cancelled"
                    ? t("salesReturns.cancelledInfo")
                    : r.state === "posted"
                      ? t("salesReturns.postedInfo")
                      : t("salesReturns.nothingToDo")
              }
            />
          ) : null}
        </SideCard>

        <SideCard id="sr-facts" title={t("salesReturns.details")}>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact
              label={t("salesReturns.kind")}
              value={word("kinds", r.kind)}
            />
            <Fact
              label={t("salesReturns.customer")}
              value={r.relation_name ?? t("salesReturns.anonymous")}
            />
            <Fact
              label={t("salesReturns.date")}
              value={formatDay(r.accounting_date, locale)}
            />
            {r.net != null ? (
              <Fact
                label={t("salesReturns.net")}
                value={bookAmount(r.net, locale)}
              />
            ) : null}
            {r.tax != null ? (
              <Fact
                label={t("salesReturns.tax")}
                value={bookAmount(r.tax, locale)}
              />
            ) : null}
            {r.rounding != null ? (
              <Fact
                label={t("salesReturns.rounding")}
                value={bookAmount(r.rounding, locale)}
              />
            ) : null}
            {r.credit != null ? (
              <Fact
                label={t("salesReturns.credit")}
                value={bookAmount(r.credit, locale)}
              />
            ) : null}
            <Fact
              label={t("salesReturns.created")}
              value={formatDateTime(r.created_at, locale)}
            />
            {r.posted_at ? (
              <Fact
                label={t("salesReturns.postedAt")}
                value={formatDateTime(r.posted_at, locale)}
              />
            ) : null}
            {r.reversed_at ? (
              <Fact
                label={t("salesReturns.reversedAt")}
                value={formatDateTime(r.reversed_at, locale)}
              />
            ) : null}
            {r.cancelled_at ? (
              <Fact
                label={t("salesReturns.cancelledAt")}
                value={formatDateTime(r.cancelled_at, locale)}
              />
            ) : null}
          </SubTile>
          {(r.reversal_reason ?? r.cancel_reason) ? (
            <p className="text-[13px] break-words">
              {r.reversal_reason ?? r.cancel_reason}
            </p>
          ) : null}
          <nav
            aria-label={t("salesReturns.sources")}
            className="flex flex-col gap-1 text-[13px]"
          >
            {r.order_id ? (
              <Link className={link} href={`/orders/${r.order_id}`}>
                {t("salesReturns.openOrder")}
              </Link>
            ) : null}
            {r.credit_invoice_id ? (
              <Link className={link} href={`/invoices/${r.credit_invoice_id}`}>
                {t("salesReturns.openCreditNote")}
              </Link>
            ) : null}
          </nav>
        </SideCard>

        <Tile
          id="sr-restocks"
          title={t("salesReturns.restocks")}
          span="col-span-8 max-md:row-span-1"
          flush
        >
          {r.restocks.length === 0 ? (
            <div className="px-6 py-6 max-md:px-4">
              <Notice
                title={
                  r.lines.some((l) => !l.origin_warehouse_id)
                    ? t("salesReturns.noStockLines")
                    : t("salesReturns.noRestocks")
                }
              />
            </div>
          ) : (
            <ul className="flex flex-col">
              {r.restocks.map((s) => (
                <li
                  key={s.id}
                  className="flex flex-wrap items-center justify-between gap-3 border-t border-(--hair) px-6 py-4 first:border-t-0 max-md:px-4"
                >
                  <div className="flex min-w-0 flex-col gap-0.5 text-[13px]">
                    <span className="font-semibold">
                      {t("salesReturns.restockInto", {
                        warehouse: s.warehouse_name,
                      })}
                    </span>
                    <span className="text-muted-foreground">
                      {formatDay(s.accounting_date, locale)} ·{" "}
                      {s.lines
                        .map((x) => {
                          const line = r.lines.find(
                            (l) => l.id === x.return_line_id
                          )
                          // Cost is null without accounting.read: quantity only.
                          return `${line ? label(line) : ""} ${formatQty(x.quantity, locale)}${x.value ? ` (${bookAmount(x.value, locale)})` : ""}`
                        })
                        .join(", ")}
                    </span>
                    {s.reversal_reason ? (
                      <span className="break-words">{s.reversal_reason}</span>
                    ) : null}
                  </div>
                  <span className="flex items-center gap-2">
                    <StatusBadge status={s.state} {...badge(s.state)} />
                    {mayUnrestock && s.allowed_actions.includes("reverse") ? (
                      <RestockReverseDialog
                        ret={r}
                        restock={s}
                        outcome={outcome}
                        button={
                          <Button
                            variant="outline"
                            size="sm"
                            className="max-md:min-h-11"
                          />
                        }
                      />
                    ) : null}
                  </span>
                </li>
              ))}
            </ul>
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
      <span className="text-right break-words">{value}</span>
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
