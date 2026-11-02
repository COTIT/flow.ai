import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { formatMoney } from "@genesy/ui/lib/money"

import { ClearCart } from "@/components/clear-cart"
import { LocalTime } from "@/components/local-time"
import { PayButton } from "@/components/pay-button"
import { RefreshWhileUnpaid } from "@/components/refresh-while-unpaid"
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
          <div className="rounded-lg border border-teal bg-teal/15 p-4">
            <p className="font-medium">{t("paid")}</p>
            <ClearCart />
          </div>
        ) : order.status === "cancelled" ? (
          <div className="flex flex-col items-start gap-3">
            <p>{t("cancelled")}</p>
            <Link href="/cart" className={buttonVariants()}>
              {t("again")}
            </Link>
          </div>
        ) : order.status === "awaiting_payment" ? (
          <p>
            {t("awaiting")}
            {order.expires_at ? (
              <>
                {" "}
                {t.rich("heldUntil", {
                  time: () => <LocalTime iso={order.expires_at!} />,
                })}
              </>
            ) : null}
          </p>
        ) : (
          <p>{t("status", { status: order.status })}</p>
        )}
      </div>

      <dl className="grid grid-cols-[1fr_auto] gap-x-6 gap-y-2 rounded-lg border p-4 tabular-nums">
        <dt>{t("subtotal")}</dt>
        <dd className="text-right">{money(order.subtotal)}</dd>
        <dt>{t("vat")}</dt>
        <dd className="text-right">{money(order.vat)}</dd>
        <dt className="font-semibold">{t("total")}</dt>
        <dd className="text-right font-semibold">{money(order.total)}</dd>
      </dl>

      {order.status === "awaiting_payment" ? (
        <>
          <PayButton orderId={order.id} />
          <RefreshWhileUnpaid />
        </>
      ) : null}
    </div>
  )
}
