"use client"

import { useLocale, useTranslations } from "next-intl"

import { formatDate, formatQty } from "@genesy/ui/lib/format"
import { formatMoney } from "@genesy/ui/lib/money"
import { flowStore, useFlow } from "@/lib/flow-store"
import { useAttention, useOpenOrders } from "@/lib/use-scene-data"
import { DataState, Restricted } from "./data-state"
import type { RegistryProps } from "./types"

/** A row the person can select; the agent then knows "this one" (ids only, sent by the shell). */
function Row({
  type,
  id,
  children,
}: {
  type: string
  id: string
  children: React.ReactNode
}) {
  const t = useTranslations("scenes")
  const on = useFlow(
    (s) => s.selected?.type === type && s.selected.ids.includes(id)
  )
  return (
    <li>
      <button
        type="button"
        aria-pressed={on}
        data-record={id}
        onClick={() =>
          flowStore.getState().setSelected(on ? null : { type, ids: [id] })
        }
        className={`flex min-h-11 w-full flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-xl px-3 py-2 text-left text-sm hover:bg-muted focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none ${on ? "bg-muted ring-1 ring-(--gold)" : ""}`}
      >
        {children}
        {on ? <span className="sr-only">{t("selected")}</span> : null}
      </button>
    </li>
  )
}

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
      className="flex flex-col gap-2 rounded-2xl border bg-card p-3"
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
        <ul className="flex flex-col">
          {rows.map((o) => (
            <Row key={o.id} type="order" id={o.id}>
              <span className="font-medium">{o.number}</span>
              <span className="text-xs text-muted-foreground">
                {formatDate(o.created_at, locale)}
              </span>
              <span className="ml-auto tabular-nums">
                {formatMoney(o.total, o.decimals, o.currency, locale)}
              </span>
            </Row>
          ))}
        </ul>
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
        <ul className="flex flex-col">
          {rows.map((p) => (
            <Row key={p.product_id} type="product" id={p.product_id}>
              <span className="font-medium">{p.product_name}</span>
              <span className="flex gap-3 text-xs text-muted-foreground tabular-nums">
                <span>
                  {t("available")} {formatQty(p.available, locale)}
                </span>
                <span>
                  {t("minimum")} {formatQty(p.minimum, locale)}
                </span>
                <span className="font-medium text-foreground">
                  {t("shortfall")} {formatQty(p.shortfall, locale)}
                </span>
              </span>
            </Row>
          ))}
        </ul>
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
        <ul className="flex flex-col">
          {rows.map((i) => (
            <Row key={i.id} type="invoice" id={i.id}>
              <span className="font-medium">{i.number ?? i.customer_name}</span>
              <span className="text-xs text-muted-foreground">
                {i.due_date ? formatDate(i.due_date, locale) : null}
              </span>
              <span className="ml-auto tabular-nums">
                {formatMoney(i.open, i.decimals, i.currency, locale)}
              </span>
            </Row>
          ))}
        </ul>
      </DataState>
    </Panel>
  )
}
