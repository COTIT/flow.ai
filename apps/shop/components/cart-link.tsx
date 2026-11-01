"use client"

import Link from "next/link"
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
      className={buttonVariants({
        variant: "ghost",
        size: "icon",
        className: "relative",
      })}
    >
      <ShoppingBagIcon aria-hidden />
      {count ? (
        <span
          aria-hidden
          className="absolute -top-1 -right-1 grid min-w-4 place-items-center rounded-full bg-gold px-1 text-[10px] font-bold text-gold-foreground"
        >
          {count}
        </span>
      ) : null}
    </Link>
  )
}
