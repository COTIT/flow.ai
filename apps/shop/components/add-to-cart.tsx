"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import Link from "@/components/shop-link"
import { useLocale, useTranslations } from "next-intl"
import { ShoppingBagIcon } from "lucide-react"

import type { StorefrontProduct } from "@genesy/api-client/model"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatMoney } from "@genesy/ui/lib/money"

import { defaultVariant } from "@/lib/price"
import { useCart } from "@/lib/use-cart"

import { Stepper } from "./stepper"
import { StockBadge } from "./stock-badge"

type Variant = StorefrontProduct["variants"][number]

const variantLabel = (v: Variant) =>
  v.options.map((o) => `${o.type}: ${o.value}`).join(", ") || v.sku || ""
const variantValue = (v: Variant) =>
  v.options.map((o) => o.value).join(", ") || v.sku || ""

/**
 * Mock `ShopProduct` buy tile: price and stock, sizes as buttons, stepper and Add, then what
 * happened. Prices shown are the API's; checkout's reply settles price and stock.
 */
export function AddToCart({
  product,
  soldOut,
}: {
  product: StorefrontProduct
  soldOut: boolean
}) {
  const t = useTranslations()
  const locale = useLocale()
  // `ready` comes after hydration: until then the controls are inert server HTML, where a
  // click would submit the form natively (page reload) and a choice would be reset.
  const { add, ready } = useCart()
  const [variantId, setVariantId] = React.useState(
    defaultVariant(product)?.id ?? null
  )
  const [quantity, setQuantity] = React.useState(1)
  const [added, setAdded] = React.useState(false)
  // Phones: once the Add button scrolls up out of view, a bar at the bottom keeps it at hand.
  const addRow = React.useRef<HTMLDivElement>(null)
  const formId = React.useId()
  const [scrolledPast, setScrolledPast] = React.useState(false)
  React.useEffect(() => {
    const row = addRow.current
    if (!row) return
    const watch = new IntersectionObserver(([entry]) =>
      setScrolledPast(
        !entry!.isIntersecting && entry!.boundingClientRect.bottom < 0
      )
    )
    watch.observe(row)
    return () => watch.disconnect()
  }, [])
  const variant = product.variants.find((v) => v.id === variantId)
  const money = (minor: number) =>
    formatMoney(minor, product.currency_decimals, product.currency, locale)
  const optionTypes = [
    ...new Set(product.variants.flatMap((v) => v.options.map((o) => o.type))),
  ]
  // A lone named size still shows, so the buyer knows which one they get.
  const choose = product.variants.length > 1 || optionTypes.length > 0

  return (
    <form
      id={formId}
      className="flex flex-col gap-[18px]"
      onSubmit={(e) => {
        e.preventDefault()
        add({
          productId: product.id,
          variantId,
          quantity,
          name: product.name,
          variantLabel: variant && choose ? variantLabel(variant) : null,
          priceGross: variant?.price_gross ?? product.price_gross,
          currency: product.currency,
          decimals: product.currency_decimals,
        })
        setAdded(true)
      }}
    >
      <div className="flex flex-wrap items-center justify-between gap-3">
        <span className="text-[32px] font-semibold tabular-nums">
          {money(variant?.price_gross ?? product.price_gross)}
        </span>
        <StockBadge
          product={product}
          labels={{
            inStock: t("product.inStock"),
            low: t("product.low"),
            soldOut: t("product.soldOut"),
          }}
        />
      </div>

      {choose ? (
        <div
          role="group"
          aria-labelledby="variant-label"
          className="flex flex-col gap-1.5"
        >
          <span id="variant-label" className="text-[13px] font-medium">
            {optionTypes.join(" / ") || t("cart.option")}
          </span>
          <div className="grid grid-cols-3 gap-2">
            {product.variants.map((v) => {
              const on = v.id === variantId
              return (
                <Button
                  key={v.id}
                  type="button"
                  variant="outline"
                  disabled={!ready}
                  aria-pressed={on}
                  onClick={() => setVariantId(v.id)}
                  className={
                    on
                      ? "h-14 flex-col gap-0 border-2 border-ring"
                      : "h-14 flex-col gap-0"
                  }
                >
                  <span className={on ? "font-semibold" : undefined}>
                    {variantValue(v)}
                  </span>{" "}
                  <span className="text-xs text-muted-foreground">
                    {money(v.price_gross)}
                  </span>
                </Button>
              )
            })}
          </div>
        </div>
      ) : null}

      {soldOut ? (
        <Notice variant="warning" title={t("product.soldOutTitle")}>
          {t("product.soldOutBody")}
        </Notice>
      ) : null}

      <div ref={addRow} className="flex items-center gap-3">
        <Stepper
          value={quantity}
          onChange={setQuantity}
          disabled={!ready || soldOut}
          labels={{
            less: t("cart.less", { name: product.name }),
            more: t("cart.more", { name: product.name }),
            value: t("cart.quantity"),
          }}
        />
        <Button
          type="submit"
          disabled={!ready || soldOut}
          size="lg"
          className="flex-1"
        >
          <ShoppingBagIcon aria-hidden className="size-[18px]" />
          {t("cart.add")}
        </Button>
      </div>

      {/* One live region, present from the start, so the added notice is announced. */}
      <div role="status" className="flex flex-col gap-[18px]">
        {added ? (
          <Notice
            live={false}
            variant="success"
            title={t("cart.added")}
            action={
              <Link
                href="/cart"
                data-slot="button"
                className={buttonVariants({ variant: "outline", size: "sm" })}
              >
                {t("cart.view")}
              </Link>
            }
          />
        ) : null}
        <p className="text-xs text-muted-foreground">{t("product.shipping")}</p>
      </div>

      {/* In <body>: a glass card's backdrop-filter would pin a fixed bar to the card. */}
      {ready && !soldOut && scrolledPast
        ? createPortal(
            <div
              data-sticky-buy
              role="region"
              aria-label={t("product.quickBuy")}
              className="fixed inset-x-0 bottom-0 z-40 flex items-center gap-3 border-t border-(--hair) glass-strong px-4 pt-2.5 pb-[max(env(safe-area-inset-bottom),10px)] md:hidden"
            >
              <div className="min-w-0 flex-1">
                <p className="truncate text-[13px] font-semibold">
                  {product.name}
                </p>
                <p className="text-[13px] text-muted-foreground tabular-nums">
                  {[
                    variant && choose ? variantValue(variant) : null,
                    quantity > 1 ? `× ${quantity}` : null,
                  ]
                    .filter(Boolean)
                    .join(" ")}
                </p>
              </div>
              {added ? (
                <Link
                  href="/cart"
                  data-slot="button"
                  className={buttonVariants({ variant: "outline" })}
                >
                  {t("cart.view")}
                </Link>
              ) : null}
              <Button type="submit" form={formId} size="lg">
                <ShoppingBagIcon aria-hidden className="size-[18px]" />
                {t("product.quickAdd", {
                  price: money(variant?.price_gross ?? product.price_gross),
                })}
              </Button>
            </div>,
            document.body
          )
        : null}
    </form>
  )
}
