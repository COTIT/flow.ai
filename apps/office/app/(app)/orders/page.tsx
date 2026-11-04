"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useOrdersList } from "@genesy/api-client/endpoints/orders"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@genesy/ui/components/select"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatMoney } from "@genesy/ui/lib/money"
import { formatDate } from "@genesy/ui/lib/format"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
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

  const columns = [
    { label: t("orders.number") },
    { label: t("orders.customer") },
    { label: t("orders.status") },
    { label: t("invoices.date") },
    { label: t("invoices.total"), right: true },
  ]
  const statusItems = [
    { value: "", label: t("lots.allStatuses") },
    ...orderStatuses.map((s) => ({ value: s, label: statusLabel(s) })),
  ]

  return (
    <>
      <PageHead eyebrow={t("shell.sell")} title={t("orders.title")} />
      <ListCard
        filters={
          <Select
            items={statusItems}
            value={status ?? ""}
            onValueChange={(v) => go({ status: v ?? "" })}
          >
            <SelectTrigger
              aria-label={t("orders.status")}
              className="w-[200px] max-md:w-full"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {statusItems.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
        count={
          orders.data
            ? t("orders.count", { total: orders.data.meta.total })
            : null
        }
        pager={
          orders.data?.data.length ? (
            <ListPager
              meta={orders.data.meta}
              onPage={(p) => go({ page: p })}
            />
          ) : null
        }
      >
        {orders.isPending ? (
          <ListSkeleton columns={columns} />
        ) : orders.isError ? (
          <CardState>
            <LoadError
              error={orders.error}
              onRetry={() => void orders.refetch()}
            />
          </CardState>
        ) : orders.data.data.length === 0 ? (
          <CardState>
            <Empty
              title={t("orders.empty")}
              onClear={status ? () => router.push("?") : undefined}
            />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((c) => (
                  <TableHead
                    key={c.label}
                    className={c.right ? "text-right" : undefined}
                  >
                    {c.label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {orders.data.data.map((o) => (
                <TableRow key={o.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline"
                      href={`/orders/${o.id}`}
                    >
                      {o.number}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub">{customerName(o.customer)}</TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={o.status}
                      label={statusLabel(o.status)}
                    />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {formatDate(o.created_at, locale)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {formatMoney(o.total, o.decimals, o.currency, locale)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
    </>
  )
}
