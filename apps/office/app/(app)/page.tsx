"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import {
  Archive,
  Calculator,
  ChevronRight,
  Layers,
  Plus,
  ShoppingBag,
  type LucideIcon,
} from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useLotsList } from "@genesy/api-client/endpoints/lots"
import { useOrdersList } from "@genesy/api-client/endpoints/orders"
import { useStockNegative } from "@genesy/api-client/endpoints/stock"
import {
  useTillSalesList,
  useTillShiftsList,
} from "@genesy/api-client/endpoints/till"
import { Badge } from "@genesy/ui/components/badge"
import { buttonVariants } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
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
import { CardState, ListSkeleton, PageHead } from "@/components/list"
import { LoadError } from "@/components/states"
import { StatusBadge } from "@/components/status-badge"

import { customerName } from "./orders/shared"

const CountUp = island(() => import("@/components/rb/CountUp"))

type Tone = "teal" | "gold" | "red" | "slate"

// glass.css `.chip-ic`: a 36 px tinted square holding a tone-coloured icon.
const chipTone: Record<Tone, string> = {
  teal: "bg-(--tint-success) text-(--tone-success)",
  gold: "bg-(--tint-warning) text-(--tone-warning)",
  red: "bg-(--tint-error) text-(--tone-error)",
  slate: "bg-(--tint-info) text-(--tone-info)",
}

function Chip({ icon: Icon, tone }: { icon: LucideIcon; tone: Tone }) {
  return (
    <span
      aria-hidden="true"
      className={cn(
        "inline-flex size-9 shrink-0 items-center justify-center rounded-xl",
        chipTone[tone]
      )}
    >
      <Icon className="size-[18px]" strokeWidth={1.75} />
    </span>
  )
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

/** A bento card with a labelled title row (`.card-head`). */
function Tile({
  id,
  title,
  sub,
  action,
  span,
  flush = false,
  children,
}: {
  id: string
  title: string
  sub?: string
  action?: React.ReactNode
  span: string
  flush?: boolean
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className={cn(
        span,
        "max-md:col-span-12",
        flush ? "gap-0 py-0" : "gap-4 px-6 py-6 max-md:gap-3 max-md:p-4"
      )}
    >
      <div
        className={cn(
          "flex flex-wrap items-center justify-between gap-3",
          flush && "px-6 pt-5 pb-4 max-md:px-4 max-md:pt-4 max-md:pb-3"
        )}
      >
        <div>
          <h2 id={id} className="text-[15px] font-semibold tracking-[0.01em]">
            {title}
          </h2>
          {sub ? (
            <p className="text-[13px] text-muted-foreground">{sub}</p>
          ) : null}
        </div>
        {action}
      </div>
      {children}
    </Card>
  )
}

function GhostLink({
  href,
  children,
}: {
  href: string
  children: React.ReactNode
}) {
  return (
    <Link
      href={href}
      className={buttonVariants({ variant: "ghost", size: "sm" })}
    >
      {children}
    </Link>
  )
}

/** The office home (OfficeOverview). Numbers are list totals (`page_size=1` → `meta.total`). */
export default function OverviewPage() {
  const t = useTranslations()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const on = { query: { enabled: Boolean(ws) } }
  const openOrders = useOrdersList(ws, { status: "open", page_size: 1 }, on)
  const recent = useOrdersList(ws, { page_size: 5 }, on)
  const quarantine = useLotsList(ws, { status: "quarantine", page_size: 3 }, on)
  const released = useLotsList(ws, { status: "released", page_size: 1 }, on)
  const recalled = useLotsList(ws, { status: "recalled", page_size: 1 }, on)
  const negative = useStockNegative(ws, { page_size: 1 }, on)
  const mismatch = useTillSalesList(ws, { mismatch: true, page_size: 1 }, on)
  const sales = useTillSalesList(ws, { page_size: 1 }, on)
  const shifts = useTillShiftsList(ws, { page_size: 1 }, on)
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
  ]
  const attentionPending =
    quarantine.isPending || negative.isPending || mismatch.isPending

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
            <Link href="/lots?new=1" className={buttonVariants()}>
              <Plus aria-hidden="true" />
              {t("lots.new")}
            </Link>
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
        <Kpi
          href="/orders?status=open"
          icon={ShoppingBag}
          tone="teal"
          label={t("overview.ordersToPick")}
          query={openOrders}
        />
        <Kpi
          href="/lots?status=quarantine"
          icon={Layers}
          tone="gold"
          label={t("overview.lotsInQuarantine")}
          query={quarantine}
        />
        <Kpi
          href="/stock/negative"
          icon={Archive}
          tone="red"
          label={t("overview.belowZero")}
          query={negative}
        />
        <Kpi
          href="/till/sales?mismatch=true"
          icon={Calculator}
          tone="slate"
          label={t("overview.mismatches")}
          query={mismatch}
        />

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
                        {o.created_at.slice(0, 10)}
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
        </Tile>

        <Tile
          id="overview-till"
          title={t("overview.till")}
          action={
            <GhostLink href="/till/shifts">{t("till.tabs.shifts")}</GhostLink>
          }
          span="col-span-4"
        >
          <div className="grid grid-cols-2 gap-2.5">
            {[
              { label: t("overview.shifts"), query: shifts },
              { label: t("overview.salesBooked"), query: sales },
            ].map(({ label, query }) => (
              <div
                key={label}
                className="rounded-2xl border border-(--hair) bg-(--glass-subtle) p-4"
              >
                <p className="text-xs text-muted-foreground">{label}</p>
                <p className="text-[22px] leading-[1.3] font-medium">
                  <Total query={query} />
                </p>
              </div>
            ))}
          </div>
          <p className="text-xs text-muted-foreground">
            {t("overview.tillNote")}
          </p>
        </Tile>

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
      </section>
    </>
  )
}
