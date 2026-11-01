"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useInvoicesList } from "@genesy/api-client/endpoints/invoices"
import { ApiError } from "@genesy/ui/components/api-error"
import { Input } from "@genesy/ui/components/input"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { StatusBadge } from "@/components/status-badge"

export default function InvoicesPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string; page?: string }>
}) {
  const t = useTranslations()
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

  return (
    <section className="flex flex-col gap-3">
      <h1 className="heading-2 text-lg">{t("invoices.title")}</h1>
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault()
          go({ q: String(new FormData(e.currentTarget).get("q") ?? "") })
        }}
      >
        <Input
          name="q"
          type="search"
          defaultValue={q}
          aria-label={t("invoices.search")}
          placeholder={t("invoices.search")}
          className="max-w-sm"
        />
      </form>
      {invoices.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : invoices.isError ? (
        <ApiError error={invoices.error} fallback={t("common.error")} />
      ) : invoices.data.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("invoices.empty")}</p>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("invoices.number")}</TableHead>
                <TableHead>{t("invoices.status")}</TableHead>
                <TableHead>{t("invoices.date")}</TableHead>
                <TableHead>{t("invoices.due")}</TableHead>
                <TableHead className="text-right">
                  {t("invoices.total")}
                </TableHead>
                <TableHead className="text-right">
                  {t("invoices.open")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {invoices.data.data.map((inv) => (
                <TableRow key={inv.id}>
                  <TableCell>
                    <Link
                      className="font-medium underline-offset-4 hover:underline"
                      href={`/invoices/${inv.id}`}
                    >
                      {inv.invoice_nr ?? t("invoices.draft")}
                    </Link>
                    {inv.is_credit ? ` · ${t("invoices.credit")}` : ""}
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={inv.status} />
                  </TableCell>
                  <TableCell>{inv.created_at.slice(0, 10)}</TableCell>
                  <TableCell>{inv.due_date?.slice(0, 10)}</TableCell>
                  {/* Amounts exactly as the API prints them; never added up here. */}
                  <TableCell className="text-right tabular-nums">
                    {inv.total_amount}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {String(inv.balance.open)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {/* No meta when the API answers a `limit` query; we never send one. */}
          {meta ? (
            <Pager
              page={meta.current_page}
              lastPage={meta.last_page}
              summary={t("pager.summary", {
                from: meta.from ?? 0,
                to: meta.to ?? 0,
                total: meta.total,
              })}
              previous={t("pager.previous")}
              next={t("pager.next")}
              onPage={(p) => go({ page: p })}
            />
          ) : null}
        </>
      )}
    </section>
  )
}
