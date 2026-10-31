"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTillSalesList } from "@genesy/api-client/endpoints/till"
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
import { useTillMoney } from "@/lib/till-money"

export default function TillSalesPage({
  searchParams,
}: {
  searchParams: Promise<{ mismatch?: string; page?: string }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { mismatch, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const { register, registerMoney } = useTillMoney()
  const sales = useTillSalesList(
    workspaceId,
    {
      mismatch: mismatch === "true" || undefined,
      page: Number(page) || undefined,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { mismatch?: boolean; page?: number }) => {
    const params = new URLSearchParams()
    if (next.mismatch ?? mismatch === "true") params.set("mismatch", "true")
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  return (
    <>
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          // Uncontrolled so the tick shows at once; the key resyncs it when the URL changes.
          key={mismatch ?? ""}
          defaultChecked={mismatch === "true"}
          onChange={(e) => go({ mismatch: e.target.checked })}
        />
        {t("till.onlyMismatch")}
      </label>
      {sales.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : sales.isError ? (
        <ApiError error={sales.error} fallback={t("common.error")} />
      ) : sales.data.data.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("till.noSales")}</p>
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("orders.number")}</TableHead>
                <TableHead>{t("till.register")}</TableHead>
                <TableHead>{t("invoices.date")}</TableHead>
                <TableHead>{t("invoices.method")}</TableHead>
                <TableHead className="text-right">
                  {t("till.charged")}
                </TableHead>
                <TableHead className="text-right">
                  {t("till.catalog")}
                </TableHead>
                <TableHead className="text-right">
                  {t("till.mismatch")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {sales.data.data.map((s) => (
                <TableRow key={s.order_id}>
                  <TableCell>
                    <Link
                      className="font-medium underline-offset-4 hover:underline"
                      href={`/orders/${s.order_id}`}
                    >
                      {s.number}
                    </Link>
                  </TableCell>
                  <TableCell>{register(s.register_id)?.name}</TableCell>
                  <TableCell className="tabular-nums">
                    {new Date(s.sold_at).toLocaleString()}
                  </TableCell>
                  <TableCell>{s.method}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {registerMoney(s.total, s.register_id)}
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {registerMoney(s.server_total, s.register_id)}
                  </TableCell>
                  <TableCell
                    className={`text-right tabular-nums ${s.price_mismatch || s.mismatched_lines ? "text-destructive" : ""}`}
                  >
                    {s.price_mismatch || s.mismatched_lines
                      ? t("till.mismatchValue", {
                          amount: registerMoney(
                            s.price_mismatch,
                            s.register_id
                          ),
                          lines: s.mismatched_lines,
                        })
                      : "—"}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Pager
            page={sales.data.meta.current_page}
            lastPage={sales.data.meta.last_page}
            summary={t("pager.summary", {
              from: sales.data.meta.from ?? 0,
              to: sales.data.meta.to ?? 0,
              total: sales.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={(p) => go({ page: p })}
          />
        </>
      )}
    </>
  )
}
