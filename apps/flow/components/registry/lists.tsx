"use client"

import { useLocale, useTranslations } from "next-intl"

import { formatDate, formatQty } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { useAttention, useOpenOrders } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import { RecordTable } from "./record-table"
import type { RegistryProps } from "./types"

function Panel({
  title,
  children,
}: {
  title: string
  children: React.ReactNode
}) {
  return (
    <section
      aria-label={title}
      className="@container flex flex-col gap-2 rounded-2xl border bg-card p-3 @min-[980px]:p-5"
    >
      <h3 className="px-1 text-sm font-semibold">{title}</h3>
      {children}
    </section>
  )
}

export function OrderList({ restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const query = useOpenOrders(!restricted)
  if (restricted) return <Restricted title={t("pendingOrders")} />
  const rows = query.data?.data ?? []
  return (
    <Panel title={t("pendingOrders")}>
      <DataState query={query} empty={!rows.length} emptyTitle={t("noOrders")}>
        <RecordTable
          label={t("pendingOrders")}
          rows={rows}
          rowKey={(o) => o.id}
          name={(o) => o.number}
          nameLabel={t("table.order")}
          select="order"
          itemAttr="data-row"
          columns={[
            {
              key: "date",
              label: t("table.date"),
              value: (o) => formatDate(o.created_at, locale),
            },
            {
              key: "total",
              label: t("table.total"),
              numeric: true,
              strong: true,
              value: (o) =>
                formatMoney(o.total, o.decimals, o.currency, locale),
            },
          ]}
        />
      </DataState>
    </Panel>
  )
}

export function StockList({ restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const query = useAttention()
  if (restricted || query.data?.data.stock === null)
    return <Restricted title={t("stockTitle")} />
  const rows = query.data?.data.stock?.below_minimum.items ?? []
  return (
    <Panel title={t("stockTitle")}>
      <DataState query={query} empty={!rows.length} emptyTitle={t("noStock")}>
        <RecordTable
          label={t("stockTitle")}
          rows={rows}
          rowKey={(p) => p.product_id}
          name={(p) => p.product_name}
          nameLabel={t("table.product")}
          select="product"
          itemAttr="data-row"
          columns={[
            {
              key: "available",
              label: t("available"),
              numeric: true,
              value: (p) => formatQty(p.available, locale),
            },
            {
              key: "minimum",
              label: t("minimum"),
              numeric: true,
              value: (p) => formatQty(p.minimum, locale),
            },
            {
              key: "shortfall",
              label: t("shortfall"),
              numeric: true,
              strong: true,
              value: (p) => formatQty(p.shortfall, locale),
            },
          ]}
        />
      </DataState>
    </Panel>
  )
}

export function InvoiceTable({ restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const query = useAttention()
  if (restricted || query.data?.data.invoices === null)
    return <Restricted title={t("invoices")} />
  const rows = query.data?.data.invoices?.items ?? []
  return (
    <Panel title={t("invoices")}>
      <DataState
        query={query}
        empty={!rows.length}
        emptyTitle={t("noInvoices")}
      >
        <RecordTable
          label={t("invoices")}
          rows={rows}
          rowKey={(i) => i.id}
          name={(i) => i.number ?? i.customer_name ?? ""}
          nameLabel={t("table.invoice")}
          select="invoice"
          itemAttr="data-row"
          columns={[
            {
              key: "due",
              label: t("table.due"),
              value: (i) => (i.due_date ? formatDate(i.due_date, locale) : ""),
            },
            {
              key: "open",
              label: t("table.open"),
              numeric: true,
              strong: true,
              value: (i) => formatMoney(i.open, i.decimals, i.currency, locale),
            },
          ]}
        />
      </DataState>
    </Panel>
  )
}
