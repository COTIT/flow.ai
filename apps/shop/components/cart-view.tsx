"use client"

import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { MinusIcon, PlusIcon, Trash2Icon } from "lucide-react"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { formatMoney } from "@genesy/ui/lib/money"

import { useCart } from "@/lib/use-cart"

export function CartView() {
  const t = useTranslations("cart")
  const locale = useLocale()
  const { lines, ready, setQuantity } = useCart()
  if (!ready) return null
  if (!lines.length)
    return (
      <EmptyState
        title={t("empty")}
        description={t("emptyWhy")}
        action={
          <Link href="/" className={buttonVariants()}>
            {t("browse")}
          </Link>
        }
      />
    )

  return (
    <div className="flex flex-col gap-6">
      <ul className="divide-y rounded-lg border">
        {lines.map((l) => (
          <li
            key={`${l.productId}:${l.variantId}`}
            className="flex flex-wrap items-center gap-3 p-4"
          >
            <div className="mr-auto flex min-w-0 flex-col">
              <span className="font-medium">{l.name}</span>
              {l.variantLabel ? (
                <span className="text-sm text-muted-foreground">
                  {l.variantLabel}
                </span>
              ) : null}
              <span className="text-sm tabular-nums">
                {t("each", {
                  price: formatMoney(
                    l.priceGross,
                    l.decimals,
                    l.currency,
                    locale
                  ),
                })}
              </span>
            </div>
            <div className="flex items-center gap-1">
              <Button
                variant="outline"
                size="icon"
                aria-label={t("less", { name: l.name })}
                onClick={() =>
                  setQuantity(l.productId, l.variantId, l.quantity - 1)
                }
              >
                <MinusIcon />
              </Button>
              <span
                className="w-10 text-center tabular-nums"
                aria-label={t("quantity")}
              >
                {l.quantity}
              </span>
              <Button
                variant="outline"
                size="icon"
                aria-label={t("more", { name: l.name })}
                onClick={() =>
                  setQuantity(l.productId, l.variantId, l.quantity + 1)
                }
              >
                <PlusIcon />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                aria-label={t("remove", { name: l.name })}
                onClick={() => setQuantity(l.productId, l.variantId, 0)}
              >
                <Trash2Icon />
              </Button>
            </div>
          </li>
        ))}
      </ul>
      {/* No sums here: prices, VAT and stock are settled by the order the server creates. */}
      <p className="text-muted-foreground">{t("totalAtCheckout")}</p>
      <Link
        href="/checkout"
        className={buttonVariants({ size: "lg", className: "w-fit" })}
      >
        {t("checkout")}
      </Link>
    </div>
  )
}
