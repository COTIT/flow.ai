"use client"

import Link from "@/components/shop-link"
import { useLocale, useTranslations } from "next-intl"
import { ShoppingBagIcon, Trash2Icon } from "lucide-react"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { formatMoney } from "@genesy/ui/lib/money"
import { cn } from "@genesy/ui/lib/utils"

import { cartCount } from "@/lib/cart"
import { card, cardTitle, honey, tile } from "@/lib/look"
import { useCart } from "@/lib/use-cart"

import { Stepper } from "./stepper"

const row = "flex flex-wrap items-center gap-4 px-5 py-[18px]"
const thumb = cn(honey, "size-[72px] shrink-0 rounded-[16px]")

/** Mock `ShopStates` "Empty cart": the empty state inside a card. Also used by checkout. */
export function EmptyCart() {
  const t = useTranslations("cart")
  return (
    <div className={card}>
      <EmptyState
        icon={<ShoppingBagIcon aria-hidden="true" className="size-[22px]" />}
        title={t("empty")}
        description={t("emptyWhy")}
        action={
          <Link
            href="/shop"
            data-slot="button"
            className={buttonVariants({ size: "sm" })}
          >
            {t("browse")}
          </Link>
        }
      />
    </div>
  )
}

/**
 * Mock `ShopCart`: the lines in one tile, the summary beside it. Unit prices only: totals,
 * VAT and stock are settled by the order the server creates. Until the cart is read from
 * the browser, skeletons hold its shape (one line, the common case).
 */
export function CartView() {
  const t = useTranslations("cart")
  const locale = useLocale()
  const { lines, ready, setQuantity } = useCart()
  if (ready && !lines.length) return <EmptyCart />

  return (
    <div className="grid grid-cols-12 gap-3 md:gap-4">
      <div
        className={cn(
          card,
          "col-span-12 gap-0 overflow-hidden p-0 md:col-span-8 md:gap-0 md:p-0"
        )}
      >
        {ready ? (
          <ul aria-label={t("title")}>
            {lines.map((l) => (
              <li
                key={`${l.productId}:${l.variantId}`}
                className={cn(row, "border-b border-(--hair) last:border-b-0")}
              >
                <span aria-hidden className={thumb} />
                <div className="min-w-0 flex-auto max-md:basis-[calc(100%-88px)]">
                  <p className="font-semibold">
                    {l.name}
                    {l.variantLabel ? ` · ${l.variantLabel}` : ""}
                  </p>
                  <p className="text-[13px] text-muted-foreground tabular-nums">
                    {t("each", {
                      price: formatMoney(
                        l.priceGross,
                        l.decimals,
                        l.currency,
                        locale
                      ),
                    })}
                  </p>
                </div>
                <Stepper
                  value={l.quantity}
                  min={0}
                  onChange={(q) => setQuantity(l.productId, l.variantId, q)}
                  labels={{
                    less: t("less", { name: l.name }),
                    more: t("more", { name: l.name }),
                    value: t("quantity"),
                  }}
                />
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={t("remove", { name: l.name })}
                  onClick={() => setQuantity(l.productId, l.variantId, 0)}
                >
                  <Trash2Icon aria-hidden />
                </Button>
              </li>
            ))}
          </ul>
        ) : (
          <div aria-hidden className={row}>
            <Skeleton className={thumb} />
            <div className="flex min-w-0 flex-auto flex-col gap-2 max-md:basis-[calc(100%-88px)]">
              <Skeleton className="h-3.5 w-3/5" />
              <Skeleton className="h-3.5 w-1/4" />
            </div>
            <Skeleton className="h-[46px] w-[122px] rounded-[12px] md:h-11" />
            <Skeleton className="size-10 rounded-[12px] max-md:size-11" />
          </div>
        )}
      </div>

      <section
        aria-labelledby="cart-summary"
        className={cn(card, "col-span-12 md:col-span-4")}
      >
        <h2 id="cart-summary" className={cardTitle}>
          {t("summary")}
        </h2>
        <div className="flex items-center justify-between text-[13px]">
          <span className="text-muted-foreground">{t("items")}</span>
          {ready ? (
            <span className="tabular-nums">{cartCount(lines)}</span>
          ) : (
            <Skeleton className="h-3.5 w-4" />
          )}
        </div>
        <p className={cn(tile, "text-[13px] text-muted-foreground")}>
          {t("totalAtCheckout")}
        </p>
        <Link
          href="/checkout"
          data-slot="button"
          className={buttonVariants({ size: "lg", className: "w-full" })}
        >
          {t("checkout")}
        </Link>
      </section>
    </div>
  )
}
