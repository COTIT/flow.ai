"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import { useAccountingInventoryValuation } from "@verzo/api-client/endpoints/accounting"
import { Button } from "@verzo/ui/components/button"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { formatDay, formatQty } from "@verzo/ui/lib/format"
import { Field } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { WarehousePicker } from "@/components/pickers"
import { Empty, LoadError } from "@/components/states"

import { ReportError, useMoney } from "../../accounting/ledger"
import { useBookMonth } from "../../accounting/shared"
import { StockPills } from "../counts/shared"

type Query = { as_of?: string; warehouse?: string }

/** Zero as the API writes a decimal string ("0", "0.00", "-0.00"). */
const isZero = (amount: string) => /^-?0(\.0+)?$/.test(amount)

/**
 * Inventory valuation (Template List): stock value per product and warehouse on a day, and,
 * for all warehouses, the GL inventory balance and its difference. Figures are the API's.
 */
export default function StockValuationPage({
  searchParams,
}: {
  searchParams: Promise<Query>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const q = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const book = useBookMonth()
  const asOf = q.as_of ?? book.today ?? ""
  const [warehouse, setWarehouse] = React.useState(q.warehouse ?? "")
  const report = useAccountingInventoryValuation(
    workspaceId,
    { as_of: asOf, warehouse_id: q.warehouse || undefined },
    { query: { enabled: Boolean(workspaceId && asOf) } }
  )
  const r = report.data?.data
  const money = useMoney(r?.functional_currency)
  const go = (next: Query) => {
    const params = new URLSearchParams()
    for (const [k, v] of Object.entries({ ...q, ...next }))
      if (v) params.set(k, v)
    router.push(`?${params}`)
  }
  const columns = [
    { label: t("valuation.product") },
    { label: t("valuation.warehouse") },
    { label: t("valuation.quantity"), right: true },
    { label: t("valuation.value"), right: true },
  ]

  if (book.setup.isError)
    return (
      <LoadError
        error={book.setup.error}
        onRetry={() => void book.setup.refetch()}
      />
    )

  return (
    <>
      <PageHead
        eyebrow={t("shell.sell")}
        title={t("valuation.title")}
        description={t("valuation.lead")}
      />
      <ListCard
        filters={
          <div className="flex w-full flex-col gap-3">
            <StockPills view="valuation" />
            <form
              key={asOf}
              className="flex flex-wrap items-end gap-3 max-md:flex-col max-md:items-stretch"
              onSubmit={(e) => {
                e.preventDefault()
                const f = new FormData(e.currentTarget)
                go({
                  as_of: String(f.get("as_of") ?? "") || undefined,
                  warehouse: warehouse || undefined,
                })
              }}
            >
              <Field id="vl-as-of" label={t("valuation.asOf")}>
                <Input type="date" name="as_of" required defaultValue={asOf} />
              </Field>
              <div className="min-w-56 max-md:w-full">
                <Field id="vl-warehouse" label={t("valuation.warehouse")}>
                  <WarehousePicker
                    value={warehouse}
                    onChange={setWarehouse}
                    placeholder={t("valuation.allWarehouses")}
                  />
                </Field>
              </div>
              {warehouse ? (
                <Button
                  type="button"
                  variant="ghost"
                  className="min-h-11"
                  onClick={() => {
                    setWarehouse("")
                    go({ warehouse: undefined })
                  }}
                >
                  {t("valuation.allWarehouses")}
                </Button>
              ) : null}
              <Button type="submit" className="min-h-11">
                {t("valuation.show")}
              </Button>
            </form>
          </div>
        }
      >
        {!asOf || report.isPending ? (
          <ListSkeleton columns={columns} />
        ) : report.isError ? (
          <ReportError error={report.error} />
        ) : !r ? null : (
          <>
            <div className="flex flex-col gap-2 px-6 pb-3 max-md:px-4">
              <p className="text-[13px] text-muted-foreground">
                {t("valuation.on", { day: formatDay(r.as_of, locale) })}
                {r.cutover_date
                  ? ` · ${t("valuation.cutover", { day: formatDay(r.cutover_date, locale) })}`
                  : ""}
              </p>
              {r.difference != null && !isZero(r.difference) ? (
                <Notice
                  variant="warning"
                  title={t("valuation.differs", {
                    difference: money(r.difference),
                  })}
                >
                  {t("valuation.differsText")}
                </Notice>
              ) : null}
              {r.warehouse_id ? (
                <Notice variant="info" title={t("valuation.oneWarehouse")} />
              ) : null}
            </div>
            {r.rows.length === 0 ? (
              <CardState>
                <Empty title={t("valuation.empty")} />
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
                  {r.rows.map((row) => (
                    <TableRow key={`${row.product.id}/${row.warehouse.id}`}>
                      <TableCell className="font-semibold">
                        {row.product.name}
                      </TableCell>
                      <TableCell data-m="sub">{row.warehouse.name}</TableCell>
                      <TableCell className="text-right tabular-nums">
                        {formatQty(row.quantity, locale)}
                      </TableCell>
                      <TableCell className="text-right tabular-nums">
                        {money(row.value)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
                <TableFooter>
                  <TableRow>
                    <TableCell colSpan={3}>{t("valuation.total")}</TableCell>
                    <TableCell className="text-right font-semibold tabular-nums">
                      {money(r.total_value)}
                    </TableCell>
                  </TableRow>
                  {r.gl_balance != null ? (
                    <TableRow>
                      <TableCell colSpan={3}>{t("valuation.gl")}</TableCell>
                      <TableCell className="text-right tabular-nums">
                        {money(r.gl_balance)}
                      </TableCell>
                    </TableRow>
                  ) : null}
                  {r.difference != null ? (
                    <TableRow>
                      <TableCell colSpan={3}>
                        {t("valuation.difference")}
                      </TableCell>
                      <TableCell className="text-right font-semibold tabular-nums">
                        {money(r.difference)}
                      </TableCell>
                    </TableRow>
                  ) : null}
                </TableFooter>
              </Table>
            )}
          </>
        )}
      </ListCard>
    </>
  )
}
