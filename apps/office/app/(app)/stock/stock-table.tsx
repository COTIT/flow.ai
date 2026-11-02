"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"

import type { PaginatedStockResource } from "@genesy/api-client/model"
import { Badge } from "@genesy/ui/components/badge"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
  PillLinks,
} from "@/components/list"
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
  const columns = [
    { label: t("products.name") },
    { label: t("stock.warehouse") },
    { label: t("stock.level"), right: true },
    { label: t("stock.reserved"), right: true },
  ]
  return (
    <>
      <PageHead eyebrow={t("shell.sell")} title={t("stock.title")} />
      <ListCard
        filters={
          <PillLinks
            label={t("stock.title")}
            links={[
              { href: "/stock", label: t("stock.all"), active: view === "all" },
              {
                href: "/stock/negative",
                label: t("stock.negative"),
                active: view === "negative",
              },
            ]}
          />
        }
        count={
          query.data ? t("stock.count", { total: query.data.meta.total }) : null
        }
        pager={
          query.data?.data.length ? (
            <ListPager meta={query.data.meta} onPage={onPage} />
          ) : null
        }
      >
        {view === "negative" ? (
          <p className="px-6 pb-4 text-[13px] text-muted-foreground max-md:px-4 max-md:pb-3">
            {t("stock.negativeHint")}
          </p>
        ) : null}
        {query.isPending ? (
          <ListSkeleton columns={columns} />
        ) : query.error || !query.data ? (
          <CardState>
            <LoadError error={query.error} onRetry={() => void query.refetch()} />
          </CardState>
        ) : query.data.data.length === 0 ? (
          <CardState>
            <Empty
              title={
                view === "negative" ? t("stock.noNegative") : t("stock.empty")
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
              {query.data.data.map((s) => {
                const product = s.relationships.product
                return (
                  <TableRow key={s.id}>
                    <TableCell>
                      {product ? (
                        <Link
                          className="font-semibold underline-offset-4 hover:underline"
                          href={`/products/${product.id}`}
                        >
                          {product.name}
                        </Link>
                      ) : null}
                    </TableCell>
                    <TableCell data-m="sub" className="text-muted-foreground">
                      {s.relationships.warehouse?.name}
                    </TableCell>
                    {/* Levels as the API prints them; the sign is read, never computed. */}
                    <TableCell
                      data-m="right"
                      className="text-right font-semibold tabular-nums"
                    >
                      {s.stock_lvl.startsWith("-") ? (
                        <Badge variant="red" data-dot="false">
                          {s.stock_lvl}
                        </Badge>
                      ) : (
                        s.stock_lvl
                      )}
                    </TableCell>
                    <TableCell data-m="hide" className="text-right tabular-nums">
                      {s.reserved}
                    </TableCell>
                  </TableRow>
                )
              })}
            </TableBody>
          </Table>
        )}
      </ListCard>
    </>
  )
}
