"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import {
  Archive,
  Calculator,
  ChevronRight,
  Layers,
  Link as LinkIcon,
  Plus,
  ShoppingBag,
  Truck,
  type LucideIcon,
} from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useUsersMe } from "@genesy/api-client/endpoints/users"
import { useLotsList } from "@genesy/api-client/endpoints/lots"
import {
  useOrdersList,
  useOrdersSalesSummary,
} from "@genesy/api-client/endpoints/orders"
import { useStockNegative } from "@genesy/api-client/endpoints/stock"
import { useSupplyChainList } from "@genesy/api-client/endpoints/supply-chain"
import { useSupplyLinksList } from "@genesy/api-client/endpoints/supply-links"
import {
  useTillSalesList,
  useTillShiftsList,
} from "@genesy/api-client/endpoints/till"
import { Badge } from "@genesy/ui/components/badge"
import { buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { island } from "@genesy/ui/components/island"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatMoney } from "@genesy/ui/lib/money"
import { cn } from "@genesy/ui/lib/utils"
import { formatDate } from "@genesy/ui/lib/format"
import { Chip, GhostLink, Metric, Tile, type Tone } from "@/components/bento"
import { CardState, ListSkeleton, PageHead } from "@/components/list"
import { LoadError } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"
import { useCan } from "@/lib/can"
import { useTillMoney } from "@/lib/till-money"

import { customerName } from "./orders/shared"

const CountUp = island(() => import("@/components/rb/CountUp"))

/** The browser's local midnights: today, tomorrow, this Monday and the next. */
function bounds() {
  const at = (days: number) => {
    const d = new Date()
    d.setHours(0, 0, 0, 0)
    d.setDate(d.getDate() + days)
    return d.toISOString()
  }
  const sinceMonday = (new Date().getDay() + 6) % 7
  return {
    today: at(0),
    tomorrow: at(1),
    monday: at(-sinceMonday),
    nextMonday: at(7 - sinceMonday),
  }
}

/** A list's total: the API counted it; the browser only shows it. */
type Count = {
  data?: { meta: { total: number } }
  isPending: boolean
  isError: boolean
}

function Total({ query, className }: { query: Count; className?: string }) {
  if (query.isPending)
    // One line of the number's own font: the same height as the number that replaces it.
    return (
      <Skeleton
        className={cn("inline-block h-[1lh] w-[1.6em] align-top", className)}
      />
    )
  if (!query.data) return <span className={className}>—</span>
  const n = query.data.meta.total
  return (
    <span className={cn("tabular-nums", className)}>
      <CountUp to={n} duration={1} trigger="load" plain={<>{n}</>} />
    </span>
  )
}

function Kpi({
  href,
  icon,
  tone,
  label,
  query,
}: {
  href: string
  icon: LucideIcon
  tone: Tone
  label: string
  query: Count
}) {
  return (
    <Link
      href={href}
      data-slot="card"
      className="col-span-3 flex flex-col gap-2 p-6 outline-none focus-visible:ring-3 focus-visible:ring-ring max-md:col-span-6 max-md:p-4"
    >
      <span className="flex items-center justify-between">
        <Chip icon={icon} tone={tone} />
        <ChevronRight
          aria-hidden="true"
          className="size-4 text-muted-foreground"
          strokeWidth={1.75}
        />
      </span>
      <span className="block text-[34px] leading-[1.1] font-semibold tracking-normal max-md:text-[28px]">
        <Total query={query} />
      </span>
      <span className="text-[13px] text-muted-foreground">{label}</span>
    </Link>
  )
}

/** The office home (OfficeOverview). Numbers are list totals (`page_size=1` → `meta.total`). */
export default function OverviewPage() {
  const t = useTranslations()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  // Each list only for a member who may read it (the API answers 403 otherwise), so those
  // queries wait for /me. A tile the member may not see goes once /me has answered.
  const meRead = useUsersMe().isSuccess
  const may = {
    orders: useCan("order.list"),
    stock: useCan("stock.list"),
    till: useCan("cashregister.list"),
    links: useCan("workspace_admin"),
    products: useCan("product.list"),
    changeProducts: useCan("product.update"),
  }
  const show = (allowed: boolean) => !meRead || allowed
  const on = { query: { enabled: Boolean(ws) } }
  const gate = (allowed: boolean) => ({
    query: { enabled: Boolean(ws) && allowed },
  })
  const openOrders = useOrdersList(
    ws,
    { status: "open", page_size: 1 },
    gate(may.orders)
  )
  const recent = useOrdersList(ws, { page_size: 5 }, gate(may.orders))
  const quarantine = useLotsList(ws, { status: "quarantine", page_size: 3 }, on)
  const released = useLotsList(ws, { status: "released", page_size: 1 }, on)
  const recalled = useLotsList(ws, { status: "recalled", page_size: 1 }, on)
  const negative = useStockNegative(ws, { page_size: 1 }, gate(may.stock))
  const mismatch = useTillSalesList(
    ws,
    { mismatch: true, page_size: 1 },
    gate(may.till)
  )
  // "Today" and "this week" are the browser's local day and week (Monday 00:00), sent as instants.
  const [day] = React.useState(bounds)
  const sales = useTillSalesList(
    ws,
    { from: day.today, to: day.tomorrow, page_size: 1 },
    gate(may.till)
  )
  const shifts = useTillShiftsList(
    ws,
    { open: true, page_size: 1 },
    gate(may.till)
  )
  const staleShifts = useTillShiftsList(
    ws,
    { open: true, to: day.today, page_size: 3 },
    gate(may.till)
  )
  const links = useSupplyLinksList(
    ws,
    { status: "pending", direction: "incoming", page_size: 3 },
    gate(may.links)
  )
  const arriving = useSupplyChainList(
    ws,
    { direction: "incoming", status: "dispatched", page_size: 3 },
    on
  )
  const week = useOrdersSalesSummary(
    ws,
    { from: day.monday, to: day.nextMonday },
    gate(may.orders)
  )
  const { register } = useTillMoney(may.till)
  const all = [
    openOrders,
    recent,
    quarantine,
    released,
    recalled,
    negative,
    mismatch,
    sales,
    shifts,
    staleShifts,
    links,
    arriving,
    week,
  ]
  const failed = all.find((q) => q.isError)

  const statusLabel = (s: string) =>
    t.has(`orders.statuses.${s}`) ? t(`orders.statuses.${s}`) : s
  const attention = [
    ...(quarantine.data?.data ?? []).map((lot) => ({
      key: lot.id,
      href: `/lots/${lot.id}`,
      icon: Layers,
      tone: "gold" as const,
      title: t("overview.lotInQuarantine", { code: lot.code }),
      sub: t("overview.lotInQuarantineHint"),
    })),
    ...(negative.data?.meta.total
      ? [
          {
            key: "negative",
            href: "/stock/negative",
            icon: Archive,
            tone: "red" as const,
            title: t("overview.belowZeroCount", {
              total: negative.data.meta.total,
            }),
            sub: t("overview.belowZeroHint"),
          },
        ]
      : []),
    ...(mismatch.data?.meta.total
      ? [
          {
            key: "mismatch",
            href: "/till/sales?mismatch=true",
            icon: Calculator,
            tone: "slate" as const,
            title: t("overview.mismatchCount", {
              total: mismatch.data.meta.total,
            }),
            sub: t("overview.mismatchHint"),
          },
        ]
      : []),
    ...(links.data?.data ?? []).map((l) => ({
      key: l.id,
      href: "/supply/links",
      icon: LinkIcon,
      tone: "teal" as const,
      title: t("overview.linkFrom", { partner: l.partner.name }),
      sub: t("overview.linkHint"),
    })),
    ...(arriving.data?.data ?? []).map((tr) => ({
      key: tr.id,
      href: "/supply/transfers",
      icon: Truck,
      tone: "slate" as const,
      title: t("overview.transferArriving", { quantity: tr.quantity }),
      sub: t("overview.transferHint", { partner: tr.partner.name }),
    })),
    ...(staleShifts.data?.data ?? []).map((s) => ({
      key: s.id,
      href: "/till/shifts?open=true",
      icon: Calculator,
      tone: "red" as const,
      title: t("overview.shiftOpenSince", {
        date: new Intl.DateTimeFormat(locale, {
          day: "numeric",
          month: "short",
        }).format(new Date(s.opened_at)),
      }),
      sub: t("overview.shiftHint", {
        register: register(s.register_id)?.name ?? "—",
      }),
    })),
  ]
  // A query that is off stays pending; only the ones this member reads count.
  const attentionPending =
    !meRead ||
    (
      [
        [quarantine, true],
        [negative, may.stock],
        [mismatch, may.till],
        [links, may.links],
        [arriving, true],
        [staleShifts, may.till],
      ] as const
    ).some(([q, allowed]) => allowed && q.isPending)

  return (
    <>
      <PageHead
        eyebrow={new Intl.DateTimeFormat(locale, {
          weekday: "long",
          day: "numeric",
          month: "long",
        }).format(new Date())}
        title={t("overview.title")}
        actions={
          <>
            <Link
              href="/supply/transfers"
              className={buttonVariants({ variant: "outline" })}
            >
              {t("overview.receiveTransfer")}
            </Link>
            {show(may.products && may.changeProducts) ? (
              <Link href="/lots?new=1" className={buttonVariants()}>
                <Plus aria-hidden="true" />
                {t("lots.new")}
              </Link>
            ) : null}
          </>
        }
      />
      {failed ? (
        <LoadError
          error={failed.error}
          onRetry={() => all.forEach((q) => q.isError && void q.refetch())}
        />
      ) : null}
      <section className="grid grid-cols-12 gap-4 max-md:gap-3">
        {show(may.orders) ? (
          <Kpi
            href="/orders?status=open"
            icon={ShoppingBag}
            tone="teal"
            label={t("overview.ordersToPick")}
            query={openOrders}
          />
        ) : null}
        <Kpi
          href="/lots?status=quarantine"
          icon={Layers}
          tone="gold"
          label={t("overview.lotsInQuarantine")}
          query={quarantine}
        />
        {show(may.stock) ? (
          <Kpi
            href="/stock/negative"
            icon={Archive}
            tone="red"
            label={t("overview.belowZero")}
            query={negative}
          />
        ) : null}
        {show(may.till) ? (
          <Kpi
            href="/till/sales?mismatch=true"
            icon={Calculator}
            tone="slate"
            label={t("overview.mismatches")}
            query={mismatch}
          />
        ) : null}

        {show(may.orders) ? (
        <Tile
          id="overview-recent"
          title={t("overview.recentOrders")}
          sub={t("overview.recentOrdersSub")}
          action={
            <GhostLink href="/orders">
              {t("overview.allOrders")}
              <ChevronRight aria-hidden="true" />
            </GhostLink>
          }
          span="col-span-8"
          flush
        >
          {/* Five rows' height whatever arrives, so the tiles below never move. */}
          <div className="min-h-[300px] max-md:min-h-[355px]">
            {recent.isPending ? (
              <ListSkeleton
                rows={5}
                rowHeight="h-6 max-md:h-[46px]"
                columns={[
                  { label: t("orders.number") },
                  { label: t("orders.customer") },
                  { label: t("orders.status") },
                  { label: t("invoices.date") },
                  { label: t("invoices.total"), right: true },
                ]}
              />
            ) : !recent.data?.data.length ? (
              <CardState>
                <EmptyState title={t("orders.empty")} />
              </CardState>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("orders.number")}</TableHead>
                    <TableHead>{t("orders.customer")}</TableHead>
                    <TableHead>{t("orders.status")}</TableHead>
                    <TableHead>{t("invoices.date")}</TableHead>
                    <TableHead className="text-right">
                      {t("invoices.total")}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {recent.data.data.map((o) => (
                    <TableRow key={o.id}>
                      <TableCell className="font-semibold">
                        <Link
                          className="underline-offset-4 hover:underline"
                          href={`/orders/${o.id}`}
                        >
                          {o.number}
                        </Link>
                      </TableCell>
                      <TableCell data-m="sub">
                        {customerName(o.customer)}
                      </TableCell>
                      <TableCell data-m="badge">
                        <StatusBadge
                          status={o.status}
                          label={statusLabel(o.status)}
                        />
                      </TableCell>
                      <TableCell
                        data-m="hide"
                        className="text-muted-foreground"
                      >
                        {formatDate(o.created_at, locale)}
                      </TableCell>
                      {/* Server amounts in minor units; formatted, never summed. */}
                      <TableCell
                        data-m="right"
                        className="text-right font-semibold tabular-nums"
                      >
                        {formatMoney(o.total, o.decimals, o.currency, locale)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </div>
        </Tile>
        ) : null}

        <Tile
          id="overview-attention"
          title={t("overview.attention")}
          action={
            attention.length ? (
              <Badge variant="gold" data-dot="false">
                {attention.length}
              </Badge>
            ) : null
          }
          span="col-span-4"
        >
          {/* From md up the rows scroll inside the card, so the row's height stays Recent orders'
              whatever arrives and the tiles below never move. */}
          <div className="relative min-h-[206px] flex-1">
            <div className="md:absolute md:-inset-[3px] md:overflow-y-auto md:p-[3px]">
              {attentionPending ? (
                <div className="flex flex-col gap-2.5">
                  {[0, 1, 2].map((i) => (
                    <Skeleton key={i} className="h-[62px] rounded-2xl" />
                  ))}
                </div>
              ) : attention.length === 0 ? (
                <EmptyState title={t("overview.allClear")} />
              ) : (
                <ul className="flex flex-col gap-2.5">
                  {attention.map((a) => (
                    <li key={a.key}>
                      <Link
                        href={a.href}
                        className="flex items-center gap-3 rounded-2xl border border-(--hair) bg-(--glass-subtle) p-3 outline-none hover:border-(--hair-2) focus-visible:ring-3 focus-visible:ring-ring"
                      >
                        <Chip icon={a.icon} tone={a.tone} />
                        <span className="min-w-0 flex-1">
                          <span className="block text-[13px] font-semibold">
                            {a.title}
                          </span>
                          <span className="block text-xs text-muted-foreground">
                            {a.sub}
                          </span>
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          </div>
        </Tile>

        {show(may.till) ? (
        <Tile
          id="overview-till"
          title={t("overview.tillToday")}
          action={
            <GhostLink href="/till/shifts?open=true">
              {t("till.tabs.shifts")}
            </GhostLink>
          }
          span="col-span-4"
        >
          <div className="grid grid-cols-2 gap-2.5">
            <Metric label={t("overview.openShifts")}>
              <Total query={shifts} />
            </Metric>
            <Metric label={t("overview.salesBooked")}>
              <Total query={sales} />
            </Metric>
          </div>
          <p className="text-xs text-muted-foreground">
            {t("overview.tillNote")}
          </p>
        </Tile>
        ) : null}

        <Tile
          id="overview-lots"
          title={t("overview.lotsByStatus")}
          action={<GhostLink href="/lots">{t("nav.lots")}</GhostLink>}
          span="col-span-4"
        >
          <div className="flex flex-col gap-2">
            {(
              [
                ["released", "teal", released],
                ["quarantine", "gold", quarantine],
                ["recalled", "red", recalled],
              ] as const
            ).map(([status, tone, query]) => (
              <div
                key={status}
                className="flex items-center justify-between text-[13px]"
              >
                <StatusBadge
                  status={status}
                  tone={tone}
                  label={t(`lots.statuses.${status}`)}
                />
                <span className="font-semibold">
                  <Total query={query} />
                </span>
              </div>
            ))}
          </div>
        </Tile>

        {/* The mockup's s-4 placeholder slot: the API's totals per currency, never summed. */}
        {show(may.orders) ? (
        <Tile
          id="overview-week"
          title={t("overview.salesWeek")}
          action={<GhostLink href="/orders">{t("nav.orders")}</GhostLink>}
          span="col-span-4"
        >
          {week.isPending ? (
            <>
              <div className="grid grid-cols-2 gap-2.5">
                {[t("overview.gross"), t("overview.refunded")].map((label) => (
                  <Metric key={label} label={label}>
                    <Skeleton className="inline-block h-[1lh] w-[4em] align-top" />
                  </Metric>
                ))}
              </div>
              <Skeleton className="h-4 w-20" />
            </>
          ) : !week.data?.data.currencies.length ? (
            <EmptyState title={t("overview.noSalesWeek")} />
          ) : (
            week.data.data.currencies.map((c) => (
              <React.Fragment key={c.currency}>
                <div className="grid grid-cols-2 gap-2.5">
                  <Metric label={t("overview.gross")}>
                    {formatMoney(c.gross_total, c.decimals, c.currency, locale)}
                  </Metric>
                  <Metric label={t("overview.refunded")}>
                    {formatMoney(
                      c.refunded_total,
                      c.decimals,
                      c.currency,
                      locale
                    )}
                  </Metric>
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("overview.weekOrders", { count: c.orders })}
                </p>
              </React.Fragment>
            ))
          )}
        </Tile>
        ) : null}
      </section>
    </>
  )
}
