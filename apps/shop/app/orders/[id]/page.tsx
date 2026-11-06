import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"

import { Badge } from "@genesy/ui/components/badge"
import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatMoney } from "@genesy/ui/lib/money"
import { cn } from "@genesy/ui/lib/utils"

import { ClearCart } from "@/components/clear-cart"
import { OrderCard } from "@/components/order-card"
import { LocalTime } from "@/components/local-time"
import { PayButton } from "@/components/pay-button"
import { PaymentWatcher } from "@/components/payment-watcher"
import { h2, tile } from "@/lib/look"
import { getOrder } from "@/lib/storefront"

type Props = { params: Promise<{ id: string }> }

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("order"))("title"),
    robots: { index: false },
  }
}

/**
 * The server's view of a web order: its lines, totals and status are the only ones shown.
 * Lines carry no picture, so each gets a neutral tile.
 */
export default async function OrderPage({ params }: Props) {
  const { id } = await params
  const [t, locale, order] = await Promise.all([
    getTranslations("order"),
    getLocale(),
    getOrder(id),
  ])
  const money = (minor: number) =>
    formatMoney(minor, order.decimals, order.currency, locale)
  const paid = order.status === "open" || order.status === "completed"
  const badge = paid
    ? { variant: "teal" as const, label: t("badge.paid") }
    : order.status === "cancelled"
      ? { variant: "red" as const, label: t("badge.cancelled") }
      : order.status === "awaiting_payment"
        ? { variant: "gold" as const, label: t("badge.awaiting") }
        : null

  return (
    <OrderCard>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="flex flex-col">
          <span className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase">
            {t("label")}{" "}
          </span>
          <span className={h2}>{order.number}</span>
        </h1>
        {badge ? <Badge variant={badge.variant}>{badge.label}</Badge> : null}
      </div>

      {/* One live region for every status, so a change (e.g. to paid) is announced. */}
      <div role="status" className="flex flex-col gap-[18px]">
        {paid ? (
          <Notice live={false} variant="success" title={t("paid")} />
        ) : order.status === "cancelled" ? (
          // Paid then cancelled: the API cancels a paid order only once it is fully refunded.
          <Notice
            live={false}
            variant={order.paid_at ? "info" : "warning"}
            title={t(order.paid_at ? "refunded" : "cancelled")}
            action={
              <Link
                href="/shop"
                data-slot="button"
                className={buttonVariants({ variant: "outline", size: "sm" })}
              >
                {t("again")}
              </Link>
            }
          >
            {t(order.paid_at ? "refundedBody" : "cancelledBody")}
          </Notice>
        ) : order.status === "awaiting_payment" ? (
          <PaymentWatcher>
            <Notice
              live={false}
              variant="info"
              title={t("awaiting")}
              action={
                <PayButton
                  orderId={order.id}
                  label={t("payAmount", { amount: money(order.total) })}
                />
              }
            >
              {order.expires_at
                ? t.rich("heldUntil", {
                    time: () => <LocalTime iso={order.expires_at!} />,
                  })
                : null}
            </Notice>
          </PaymentWatcher>
        ) : (
          <Notice live={false} title={t("status", { status: order.status })} />
        )}
      </div>
      {paid ? <ClearCart /> : null}

      <div
        className={cn(
          "grid grid-cols-1 gap-4",
          order.lines.length && "md:grid-cols-2"
        )}
      >
        {order.lines.length ? (
          <ul
            aria-label={t("items")}
            className={cn(tile, "flex flex-col gap-2")}
          >
            {order.lines.map((line, i) => (
              <li key={i} className="flex items-center gap-3">
                <span
                  aria-hidden
                  className="size-11 shrink-0 rounded-[12px] bg-(--hair)"
                />
                <div className="min-w-0 flex-auto">
                  <p className="text-[13px] font-semibold">{line.title}</p>
                  <p className="text-xs text-muted-foreground">
                    {[
                      // String in, so Intl shows the API's decimal exactly ("2.00" -> "2").
                      new Intl.NumberFormat(locale, {
                        maximumFractionDigits: 3,
                      }).format(line.quantity as unknown as number),
                      line.unit_price_gross !== null &&
                        money(line.unit_price_gross),
                    ]
                      .filter(Boolean)
                      .join(" × ")}
                  </p>
                </div>
                <span className="text-[13px] font-semibold tabular-nums">
                  {money(line.total_gross)}
                </span>
              </li>
            ))}
          </ul>
        ) : null}
        <dl
          className={cn(
            tile,
            "grid grid-cols-[1fr_auto] items-baseline gap-y-2 text-[13px] tabular-nums [&_dd]:pl-6"
          )}
        >
          <dt className="text-muted-foreground">{t("subtotal")}</dt>
          <dd className="text-right">{money(order.subtotal)}</dd>
          <dt className="text-muted-foreground">{t("vat")}</dt>
          <dd className="text-right">{money(order.vat)}</dd>
          <dt className="border-t border-(--hair) pt-2 text-sm font-semibold">
            {t("total")}
          </dt>
          <dd className="border-t border-(--hair) pt-2 text-right text-lg font-semibold max-md:text-base">
            {money(order.total)}
          </dd>
        </dl>
      </div>
    </OrderCard>
  )
}
