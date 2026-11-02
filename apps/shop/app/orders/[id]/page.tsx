import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatMoney } from "@genesy/ui/lib/money"

import { ClearCart } from "@/components/clear-cart"
import { LocalTime } from "@/components/local-time"
import { PayButton } from "@/components/pay-button"
import { PaymentWatcher } from "@/components/payment-watcher"
import { getOrder } from "@/lib/storefront"

type Props = { params: Promise<{ id: string }> }

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("order"))("title"),
    robots: { index: false },
  }
}

/** The server's view of a web order: its totals and status are the only ones shown. */
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

  return (
    <div className="flex max-w-xl flex-col gap-6">
      <h1 className="heading-1">{t("heading", { number: order.number })}</h1>

      {/* One live region for every status, so a change (e.g. to paid) is announced. */}
      <div role="status">
        {paid ? (
          <Notice live={false} variant="success" title={t("paid")} />
        ) : order.status === "cancelled" ? (
          <Notice
            live={false}
            variant="warning"
            title={t("cancelled")}
            action={
              <Link href="/" className={buttonVariants({ variant: "outline" })}>
                {t("again")}
              </Link>
            }
          >
            {t("cancelledBody")}
          </Notice>
        ) : order.status === "awaiting_payment" ? (
          <PaymentWatcher>
            <Notice live={false} variant="info" title={t("awaiting")}>
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

      <dl className="grid grid-cols-[1fr_auto] gap-x-6 gap-y-2 rounded-lg border p-4 tabular-nums">
        <dt>{t("subtotal")}</dt>
        <dd className="text-right">{money(order.subtotal)}</dd>
        <dt>{t("vat")}</dt>
        <dd className="text-right">{money(order.vat)}</dd>
        <dt className="font-semibold">{t("total")}</dt>
        <dd className="text-right font-semibold">{money(order.total)}</dd>
      </dl>

      {order.status === "awaiting_payment" ? (
        <PayButton orderId={order.id} />
      ) : null}
    </div>
  )
}
