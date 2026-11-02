import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatMoney } from "@genesy/ui/lib/money"
import { cn } from "@genesy/ui/lib/utils"

import { ClearCart } from "@/components/clear-cart"
import { OrderCard } from "@/components/order-card"
import { LocalTime } from "@/components/local-time"
import { PayButton } from "@/components/pay-button"
import { PaymentWatcher } from "@/components/payment-watcher"
import { badgeTone, h2, tile } from "@/lib/look"
import { getOrder } from "@/lib/storefront"

type Props = { params: Promise<{ id: string }> }

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("order"))("title"),
    robots: { index: false },
  }
}

/**
 * The server's view of a web order: its totals and status are the only ones shown. The
 * mockup's line items are left out: the order reply carries totals only.
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
    ? { tone: badgeTone.teal, label: t("badge.paid") }
    : order.status === "cancelled"
      ? { tone: badgeTone.red, label: t("badge.cancelled") }
      : order.status === "awaiting_payment"
        ? { tone: badgeTone.gold, label: t("badge.awaiting") }
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
        {badge ? <span className={badge.tone}>{badge.label}</span> : null}
      </div>

      {/* One live region for every status, so a change (e.g. to paid) is announced. */}
      <div role="status" className="flex flex-col gap-[18px]">
        {paid ? (
          <Notice live={false} variant="success" title={t("paid")} />
        ) : order.status === "cancelled" ? (
          <Notice
            live={false}
            variant="warning"
            title={t("cancelled")}
            action={
              <Link
                href="/"
                className={buttonVariants({ variant: "outline", size: "sm" })}
              >
                {t("again")}
              </Link>
            }
          >
            {t("cancelledBody")}
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
        <dt className="border-t border-border pt-2 text-sm font-semibold">
          {t("total")}
        </dt>
        <dd className="border-t border-border pt-2 text-right text-lg font-semibold max-md:text-base">
          {money(order.total)}
        </dd>
      </dl>
    </OrderCard>
  )
}
