import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"

import { CartView } from "@/components/cart-view"

export async function generateMetadata(): Promise<Metadata> {
  return {
    title: (await getTranslations("cart"))("title"),
    robots: { index: false },
  }
}

export default async function CartPage() {
  const t = await getTranslations("cart")
  return (
    <div className="flex flex-col gap-6">
      <h1 className="heading-1">{t("title")}</h1>
      <CartView />
    </div>
  )
}
