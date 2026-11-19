"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useSalesReturnsList } from "@genesy/api-client/endpoints/sales-returns"
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

import { bookAmount, useStateBadge, useWord } from "./shared"

/**
 * Customer returns and credits (Template List, as /stock/counts): newest first, the server's
 * credit amounts. New credits start from an invoice credit note, an order return or the Till.
 */
export default function SalesReturnsPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const badge = useStateBadge()
  const word = useWord()
  const { page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const returns = useSalesReturnsList(
    workspaceId,
    { page: page || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const columns = [
    { label: t("salesReturns.return") },
    { label: t("salesReturns.customer") },
    { label: t("salesReturns.state") },
    { label: t("salesReturns.date") },
    { label: t("salesReturns.credit"), right: true },
  ]
  const rows = returns.data?.data ?? []

  return (
    <>
      <PageHead
        eyebrow={t("shell.sell")}
        title={t("salesReturns.title")}
        description={t("salesReturns.intro")}
      />
      <ListCard
        count={
          returns.data
            ? t("salesReturns.count", { total: returns.data.meta.total })
            : null
        }
        pager={
          returns.data?.data.length ? (
            <ListPager
              meta={returns.data.meta}
              onPage={(p) => router.push(`?page=${p}`)}
            />
          ) : null
        }
      >
        {returns.isPending ? (
          <ListSkeleton columns={columns} />
        ) : returns.isError ? (
          <CardState>
            <LoadError
              error={returns.error}
              onRetry={() => void returns.refetch()}
            />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <Empty title={t("salesReturns.empty")} />
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
              {rows.map((r) => (
                <TableRow key={r.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
                      href={`/sales-returns/${r.id}`}
                    >
                      {r.order_number
                        ? t("salesReturns.ofOrder", { number: r.order_number })
                        : word("kinds", r.kind)}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub" className="max-w-[28ch] truncate">
                    {r.relation_name ?? t("salesReturns.anonymous")}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge status={r.state} {...badge(r.state)} />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {formatDay(r.accounting_date, locale)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {r.credit != null
                      ? bookAmount(r.credit, locale)
                      : t("salesReturns.notPosted")}
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
