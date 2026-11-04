"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTillSalesList } from "@genesy/api-client/endpoints/till"
import { Switch } from "@genesy/ui/components/switch"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { CardState, ListCard, ListPager, ListSkeleton } from "@/components/list"
import { useTillMoney } from "@/lib/till-money"
import { Empty, LoadError } from "@/components/states"

import { RegisterSelect } from "../register-select"

export default function TillSalesPage({
  searchParams,
}: {
  searchParams: Promise<{ mismatch?: string; register_id?: string; page?: string }>
}) {
  const t = useTranslations()
  const router = useRouter()
  const { mismatch, register_id, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const { register, registerMoney } = useTillMoney()
  const sales = useTillSalesList(
    workspaceId,
    {
      mismatch: mismatch === "true" || undefined,
      register_id: register_id || undefined,
      page: Number(page) || undefined,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { mismatch?: boolean; register?: string; page?: number }) => {
    const params = new URLSearchParams()
    if (next.mismatch ?? mismatch === "true") params.set("mismatch", "true")
    const reg = next.register ?? register_id
    if (reg) params.set("register_id", reg)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }

  const columns = [
    { label: t("orders.number") },
    { label: t("till.register") },
    { label: t("invoices.date") },
    { label: t("invoices.method") },
    { label: t("till.charged"), right: true },
    { label: t("till.catalog"), right: true },
    { label: t("till.mismatch"), right: true },
  ]

  return (
    <ListCard
      filters={
        <>
          <label className="flex min-h-11 items-center gap-3 text-sm">
            <Switch
              // Uncontrolled so it flips at once; the key resyncs it when the URL changes.
              key={mismatch ?? ""}
              defaultChecked={mismatch === "true"}
              onCheckedChange={(checked) => go({ mismatch: checked, page: 1 })}
            />
            {t("till.onlyMismatch")}
          </label>
          <RegisterSelect
            value={register_id ?? ""}
            onChange={(id) => go({ register: id, page: 1 })}
          />
        </>
      }
      count={
        sales.data
          ? t("till.salesCount", { total: sales.data.meta.total })
          : null
      }
      pager={
        sales.data?.data.length ? (
          <ListPager meta={sales.data.meta} onPage={(p) => go({ page: p })} />
        ) : null
      }
    >
      {sales.isPending ? (
        <ListSkeleton columns={columns} />
      ) : sales.isError ? (
        <CardState>
          <LoadError error={sales.error} onRetry={() => void sales.refetch()} />
        </CardState>
      ) : sales.data.data.length === 0 ? (
        <CardState>
          <Empty
            title={t("till.noSales")}
            onClear={
              mismatch === "true" || register_id
                ? () => router.push("?")
                : undefined
            }
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
            {sales.data.data.map((s) => (
              <TableRow key={s.order_id}>
                <TableCell>
                  <Link
                    className="font-mono font-semibold tracking-[0.04em] underline-offset-4 hover:underline"
                    href={`/orders/${s.order_id}`}
                  >
                    {s.number}
                  </Link>
                </TableCell>
                <TableCell data-m="sub">
                  {register(s.register_id)?.name}
                </TableCell>
                <TableCell data-m="sub" className="tabular-nums">
                  {new Date(s.sold_at).toLocaleString()}
                </TableCell>
                <TableCell data-m="hide">{s.method}</TableCell>
                <TableCell
                  data-m="right"
                  className="text-right font-semibold tabular-nums"
                >
                  {registerMoney(s.total, s.register_id)}
                </TableCell>
                <TableCell data-m="hide" className="text-right tabular-nums">
                  {registerMoney(s.server_total, s.register_id)}
                </TableCell>
                <TableCell
                  data-m="badge"
                  className={`text-right tabular-nums ${s.price_mismatch || s.mismatched_lines ? "text-destructive" : ""}`}
                >
                  {s.price_mismatch || s.mismatched_lines
                    ? t("till.mismatchValue", {
                        amount: registerMoney(s.price_mismatch, s.register_id),
                        lines: s.mismatched_lines,
                      })
                    : "—"}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </ListCard>
  )
}
