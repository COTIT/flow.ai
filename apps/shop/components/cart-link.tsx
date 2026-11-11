"use client"

import Link from "@/components/shop-link"
import { useTranslations } from "next-intl"
import { ShoppingBagIcon } from "lucide-react"

import { buttonVariants } from "@genesy/ui/components/button"

import { cartCount } from "@/lib/cart"
import { useCart } from "@/lib/use-cart"

export function CartLink() {
  const t = useTranslations("cart")
  const { lines, ready } = useCart()
  const count = ready ? cartCount(lines) : 0
  return (
    <Link
      href="/cart"
      aria-label={t("link", { count })}
      data-slot="button"
      className={buttonVariants({ variant: "outline" })}
    >
      <ShoppingBagIcon aria-hidden />
      <span className="hidden md:inline">{t("short")}</span>
      {count ? (
        <span
          aria-hidden
          className="inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-(image:--gold-gradient) px-1.5 text-[11px] font-bold text-gold-foreground"
        >
          {count}
        </span>
      ) : null}
    </Link>
  )
}
