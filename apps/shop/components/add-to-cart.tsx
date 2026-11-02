"use client"

import * as React from "react"
import Link from "next/link"
import { useTranslations } from "next-intl"

import type { StorefrontProduct } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Label } from "@genesy/ui/components/label"

import { useCart } from "@/lib/use-cart"

const variantLabel = (v: StorefrontProduct["variants"][number]) =>
  v.options.map((o) => `${o.type}: ${o.value}`).join(", ") || v.sku || ""

export function AddToCart({
  product,
  soldOut,
}: {
  product: StorefrontProduct
  soldOut: boolean
}) {
  const t = useTranslations("cart")
  // `ready` comes after hydration: until then the controls are inert server HTML, where a
  // click would submit the form natively (page reload) and a choice would be reset.
  const { add, ready } = useCart()
  const [variantId, setVariantId] = React.useState(
    product.variants[0]?.id ?? null
  )
  const [quantity, setQuantity] = React.useState(1)
  const [added, setAdded] = React.useState(false)
  const variant = product.variants.find((v) => v.id === variantId)

  return (
    <form
      className="flex flex-col gap-3"
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
      <div className="flex flex-wrap items-end gap-3">
        {product.variants.length > 1 ? (
          <div className="flex flex-col gap-1">
            <Label htmlFor="variant">{t("option")}</Label>
            <select
              disabled={!ready}
              id="variant"
              value={variantId ?? ""}
              onChange={(e) => setVariantId(e.target.value)}
              className="h-8 rounded-lg border border-input bg-background px-2 text-sm"
            >
              {product.variants.map((v) => (
                <option key={v.id} value={v.id}>
                  {variantLabel(v)}
                </option>
              ))}
            </select>
          </div>
        ) : null}
        <div className="flex w-24 flex-col gap-1">
          <Label htmlFor="quantity">{t("quantity")}</Label>
          <Input
            disabled={!ready}
            id="quantity"
            type="number"
            inputMode="numeric"
            min={1}
            max={1000}
            value={quantity}
            onChange={(e) =>
              setQuantity(Math.max(1, Math.trunc(Number(e.target.value)) || 1))
            }
          />
        </div>
        <Button type="submit" size="lg" disabled={!ready || soldOut}>
          {t("add")}
        </Button>
      </div>
      <p role="status" className="min-h-5 text-sm">
        {added ? (
          <>
            {t("added")}{" "}
            <Link href="/cart" className="underline underline-offset-4">
              {t("view")}
            </Link>
          </>
        ) : null}
      </p>
    </form>
  )
}
