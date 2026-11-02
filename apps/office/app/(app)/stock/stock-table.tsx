"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"

import type { PaginatedStockResource } from "@genesy/api-client/model"
import { Pager } from "@genesy/ui/components/pager"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Empty, LoadError } from "@/components/states"

/** Stock rows from `stock_list` or `stock_negative`, with the "all / below zero" switch. */
export function StockTable({
  view,
  query,
  onPage,
}: {
  view: "all" | "negative"
  query: {
    isPending: boolean
    error: unknown
    data?: PaginatedStockResource
    refetch: () => unknown
  }
  onPage: (page: number) => void
}) {
  const t = useTranslations()
  const tab = (href: string, active: boolean, label: string) => (
    <Link
      href={href}
      aria-current={active ? "page" : undefined}
      className="rounded-lg px-3 py-1.5 text-sm text-muted-foreground hover:bg-muted aria-[current=page]:bg-secondary aria-[current=page]:font-medium aria-[current=page]:text-foreground"
    >
      {label}
    </Link>
  )
  return (
    <section className="flex flex-col gap-3">
      <h1 className="heading-2 text-lg">{t("stock.title")}</h1>
      <div className="flex gap-1">
        {tab("/stock", view === "all", t("stock.all"))}
        {tab("/stock/negative", view === "negative", t("stock.negative"))}
      </div>
      {view === "negative" ? (
        <p className="text-sm text-muted-foreground">
          {t("stock.negativeHint")}
        </p>
      ) : null}
      {query.isPending ? (
        <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
      ) : query.error || !query.data ? (
        <LoadError error={query.error} onRetry={() => void query.refetch()} />
      ) : query.data.data.length === 0 ? (
        <Empty
          title={view === "negative" ? t("stock.noNegative") : t("stock.empty")}
        />
      ) : (
        <>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("products.name")}</TableHead>
                <TableHead>{t("stock.warehouse")}</TableHead>
                <TableHead className="text-right">{t("stock.level")}</TableHead>
                <TableHead className="text-right">
                  {t("stock.reserved")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {query.data.data.map((s) => {
                const product = s.relationships.product
                return (
                  <TableRow key={s.id}>
                    <TableCell>
                      {product ? (
                        <Link
                          className="underline-offset-4 hover:underline"
                          href={`/products/${product.id}`}
                        >
                          {product.name}
                        </Link>
                      ) : null}
                    </TableCell>
                    <TableCell>{s.relationships.warehouse?.name}</TableCell>
                    <TableCell
                      className={`text-right tabular-nums ${s.stock_lvl.startsWith("-") ? "text-destructive" : ""}`}
                    >
                      {s.stock_lvl}
                    </TableCell>
                    <TableCell className="text-right tabular-nums">
                      {s.reserved}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
          <Pager
            page={query.data.meta.current_page}
            lastPage={query.data.meta.last_page}
            summary={t("pager.summary", {
              from: query.data.meta.from ?? 0,
              to: query.data.meta.to ?? 0,
              total: query.data.meta.total,
            })}
            previous={t("pager.previous")}
            next={t("pager.next")}
            onPage={onPage}
          />
        </>
      )}
    </section>
  )
}
