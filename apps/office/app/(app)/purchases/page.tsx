"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  usePurchaseOrdersList,
} from "@genesy/api-client/endpoints/purchase-orders"
import { buttonVariants } from "@genesy/ui/components/button"
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
import { formatDay } from "@genesy/ui/lib/format"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"
import { useCan } from "@/lib/can"

import { poStates, showMoney, useWord } from "./shared"

/** Purchase orders (Template List, as /orders): state filter, server totals, pager. */
export default function PurchasesPage({
  searchParams,
}: {
  searchParams: Promise<{ state?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const word = useWord()
  const mayCreate = useCan("purchaseorder.create")
  const { state, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const orders = usePurchaseOrdersList(
    workspaceId,
    { state: state || undefined, page: page || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { state?: string; page?: number }) => {
    const params = new URLSearchParams()
    const st = next.state ?? state
    if (st) params.set("state", st)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  const columns = [
    { label: t("purchases.number") },
    { label: t("purchases.supplier") },
    { label: t("purchases.state") },
    { label: t("purchases.purchaseDate") },
    { label: t("purchases.totalOrdered"), right: true },
  ]
  const stateItems = [
    { value: "", label: t("purchases.allStates") },
    ...poStates.map((s) => ({ value: s, label: word("states", s) })),
  ]
  const create = mayCreate ? (
    <Link href="/purchases/new" className={buttonVariants()}>
      {t("purchases.new")}
    </Link>
  ) : null

  return (
    <>
      <PageHead
        eyebrow={t("shell.buy")}
        title={t("purchases.title")}
        actions={create}
      />
      <ListCard
        filters={
          <Select
            items={stateItems}
            value={state ?? ""}
            onValueChange={(v) => go({ state: v ?? "" })}
          >
            <SelectTrigger
              aria-label={t("purchases.state")}
              className="w-[200px] max-md:w-full"
            >
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {stateItems.map((o) => (
                <SelectItem key={o.value} value={o.value}>
                  {o.label}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        }
        count={
          orders.data
            ? t("purchases.count", { total: orders.data.meta.total })
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
              title={t("purchases.empty")}
              onClear={state ? () => router.push("?") : undefined}
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
                      href={`/purchases/${o.id}`}
                    >
                      {t("purchases.detailTitle", { number: o.number })}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub">{o.supplier.name ?? ""}</TableCell>
                  <TableCell data-m="badge">
                    <span className="flex flex-wrap gap-1.5">
                      <StatusBadge
                        status={o.state}
                        label={word("states", o.state)}
                      />
                      {o.state === "confirmed" ? (
                        <StatusBadge
                          status={o.receipt_status}
                          label={word("receiptStatus", o.receipt_status)}
                          tone={o.receipt_status === "received" ? "teal" : "gold"}
                        />
                      ) : null}
                    </span>
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {formatDay(o.purchase_date, locale)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {showMoney(o.totals.ordered, o.currency, locale)}
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
