"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"
import { Search } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useInvoicesList } from "@genesy/api-client/endpoints/invoices"
import { Input } from "@genesy/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
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

export default function InvoicesPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const { q, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const invoices = useInvoicesList(
    workspaceId,
    { q: q || undefined, page: Number(page) || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const meta = invoices.data?.meta
  const go = (next: { q?: string; page?: number }) => {
    const params = new URLSearchParams()
    const query = next.q ?? q
    if (query) params.set("q", query)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  const columns = [
    { label: t("invoices.number") },
    { label: t("invoices.customer") },
    { label: t("invoices.status") },
    { label: t("invoices.date") },
    { label: t("invoices.due") },
    { label: t("invoices.total"), right: true },
    { label: t("invoices.open"), right: true },
  ]

  return (
    <>
      <PageHead eyebrow={t("shell.sell")} title={t("invoices.title")} />
      <ListCard
        filters={
          <form
            role="search"
            className="relative w-full max-w-[360px]"
            onSubmit={(e) => {
              e.preventDefault()
              go({ q: String(new FormData(e.currentTarget).get("q") ?? "") })
            }}
          >
            <Search
              aria-hidden="true"
              strokeWidth={1.75}
              className="pointer-events-none absolute top-1/2 left-3.5 size-[18px] -translate-y-1/2 text-muted-foreground"
            />
            <Input
              name="q"
              type="search"
              defaultValue={q}
              aria-label={t("invoices.search")}
              placeholder={t("invoices.search")}
              className="pl-10"
            />
          </form>
        }
        count={meta ? t("invoices.count", { total: meta.total }) : null}
        // No meta when the API answers a `limit` query; we never send one.
        pager={
          meta && invoices.data?.data.length ? (
            <ListPager meta={meta} onPage={(p) => go({ page: p })} />
          ) : null
        }
      >
        {invoices.isPending ? (
          <ListSkeleton columns={columns} />
        ) : invoices.isError ? (
          <CardState>
            <LoadError
              error={invoices.error}
              onRetry={() => void invoices.refetch()}
            />
          </CardState>
        ) : invoices.data.data.length === 0 ? (
          <CardState>
            <Empty
              title={t("invoices.empty")}
              onClear={q ? () => router.push("?") : undefined}
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
              {invoices.data.data.map((inv) => (
                <TableRow key={inv.id}>
                  <TableCell>
                    <Link
                      className="font-mono font-semibold tracking-[0.04em] underline-offset-4 hover:underline"
                      href={`/invoices/${inv.id}`}
                    >
                      {inv.invoice_nr ?? t("invoices.draft")}
                    </Link>
                    {inv.is_credit ? ` · ${t("invoices.credit")}` : ""}
                  </TableCell>
                  <TableCell data-m="sub">{inv.customer_name ?? "—"}</TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={inv.status}
                      label={t(`invoices.statuses.${inv.status}`)}
                    />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {formatDate(inv.created_at, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {inv.due_date ? formatDate(inv.due_date, locale) : "—"}
                  </TableCell>
                  {/* Amounts exactly as the API prints them; never added up here. */}
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {inv.total_amount}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {String(inv.balance.open)}
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
