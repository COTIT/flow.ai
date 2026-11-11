import type { Metadata } from "next"
import Link from "@/components/shop-link"
import { getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"

import { CartView } from "@/components/cart-view"
import { h2 } from "@/lib/look"

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("cart"))("title"),
    robots: { index: false },
  }
}

/** Mock `ShopCart`: title with "Keep shopping", then the cart tiles. */
export default async function CartPage() {
  const t = await getTranslations("cart")
  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className={h2}>{t("title")}</h1>
        <Link
          href="/shop"
          data-slot="button"
          className={buttonVariants({ variant: "ghost", size: "sm" })}
        >
          {t("keepShopping")}
        </Link>
      </div>
      <CartView />
    </>
  )
}
