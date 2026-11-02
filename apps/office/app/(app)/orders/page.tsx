"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useOrdersList } from "@genesy/api-client/endpoints/orders"
import { ApiError } from "@genesy/ui/components/api-error"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatMoney } from "@genesy/ui/lib/money"
import { selectClass } from "@/components/field"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

import { customerName, orderStatuses } from "./shared"

export default function OrdersPage({
  searchParams,
}: {
  searchParams: Promise<{ status?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const { status, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const orders = useOrdersList(
    workspaceId,
    { status: status || undefined, page: Number(page) || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const statusLabel = (s: string) =>
    t.has(`orders.statuses.${s}`) ? t(`orders.statuses.${s}`) : s
  const go = (next: { status?: string; page?: number }) => {
    const params = new URLSearchParams()
    const st = next.status ?? status
    if (st) params.set("status", st)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  return (
    <section className="flex flex-col gap-3">
      <h1 className="heading-2 text-lg">{t("orders.title")}</h1>
      <select
        aria-label={t("orders.status")}
        className={`${selectClass} self-start`}
        value={status ?? ""}
        onChange={(e) => go({ status: e.target.value })}
      >
        <option value="">{t("lots.allStatuses")}</option>
        {orderStatuses.map((s) => (
          <option key={s} value={s}>
            {statusLabel(s)}
          </option>
        ))}
      </select>
      {orders.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : orders.isError ? (
        <LoadError error={orders.error} onRetry={() => void orders.refetch()} />
      ) : orders.data.data.length === 0 ? (
        <Empty
          title={t("orders.empty")}
          onClear={status ? () => router.push("?") : undefined}
        />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("orders.number")}</TableHead>
                <TableHead>{t("orders.status")}</TableHead>
                <TableHead>{t("orders.customer")}</TableHead>
                <TableHead>{t("invoices.date")}</TableHead>
                <TableHead className="text-right">
                  {t("invoices.total")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {orders.data.data.map((o) => (
                <TableRow key={o.id}>
                  <TableCell>
                    <Link
                      className="font-medium underline-offset-4 hover:underline"
                      href={`/orders/${o.id}`}
                    >
                      {o.number}
                    </Link>
                  </TableCell>
                  <TableCell>
                    <StatusBadge
                      status={o.status}
                      label={statusLabel(o.status)}
                    />
                  </TableCell>
                  <TableCell>{customerName(o.customer)}</TableCell>
                  <TableCell>{o.created_at.slice(0, 10)}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {formatMoney(o.total, o.decimals, o.currency, locale)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={orders.data.meta.current_page}
            lastPage={orders.data.meta.last_page}
            summary={t("pager.summary", {
              from: orders.data.meta.from ?? 0,
              to: orders.data.meta.to ?? 0,
              total: orders.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={(p) => go({ page: p })}
          />
        </>
      )}
    </section>
  )
}
