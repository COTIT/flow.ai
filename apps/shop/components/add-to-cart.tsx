"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { ShoppingBagIcon } from "lucide-react"

import type { StorefrontProduct } from "@genesy/api-client/model"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatMoney } from "@genesy/ui/lib/money"

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
    product.variants[0]?.id ?? null
  )
  const [quantity, setQuantity] = React.useState(1)
  const [added, setAdded] = React.useState(false)
  const variant = product.variants.find((v) => v.id === variantId)
  const money = (minor: number) =>
    formatMoney(minor, product.currency_decimals, product.currency, locale)
  const optionTypes = [
    ...new Set(product.variants.flatMap((v) => v.options.map((o) => o.type))),
  ]

  return (
    <form
      className="flex flex-col gap-[18px]"
      onSubmit={(e) => {
        e.preventDefault()
        add({
          productId: product.id,
          variantId,
          quantity,
          name: product.name,
          variantLabel:
            variant && product.variants.length > 1
              ? variantLabel(variant)
              : null,
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
          stock={product.available_stock}
          labels={{
            inStock: t("product.inStock"),
            soldOut: t("product.soldOut"),
          }}
        />
      </div>

      {product.variants.length > 1 ? (
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

      <div className="flex items-center gap-3">
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
          className="h-[52px] flex-1 gap-2 rounded-[14px] px-[22px] text-base"
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
                className={buttonVariants({ variant: "outline", size: "sm" })}
              >
                {t("cart.view")}
              </Link>
            }
          />
        ) : null}
        <p className="text-xs text-muted-foreground">{t("product.shipping")}</p>
      </div>
    </form>
  )
}
