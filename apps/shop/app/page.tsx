import { redirect } from "next/navigation"
import { getLocale, getTranslations } from "next-intl/server"

import { StorefrontPage } from "@genesy/ui/components/storefront-page"
import LegacyHomePage from "@/components/legacy-home"
import { TraceForm } from "@/components/trace-form"
import { getPublishedDesign } from "@/lib/storefront-design"
import { getShopBase } from "@/lib/shop-scope"
import { shopPath } from "@/lib/shop-path"
import ShopPage from "./shop/page"

/** Published promotional body; native passport lookup stays available in Shop. */
export default async function HomePage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const { page } = await searchParams
  const base = await getShopBase()
  if (page) redirect(shopPath(base, `/shop?page=${encodeURIComponent(page)}`))

  const design = await getPublishedDesign()
  if (!design)
    return base ? (
      <ShopPage searchParams={Promise.resolve({})} />
    ) : (
      <LegacyHomePage />
    )

  const [t, locale] = await Promise.all([getTranslations(), getLocale()])
  return (
    <>
      <StorefrontPage
        document={design.document}
        collections={design.collections}
        locale={locale}
        basePath={base}
        labels={{
          inStock: t("product.inStock"),
          low: t("product.low"),
          soldOut: t("product.soldOut"),
          imageSoon: t("product.imageSoon"),
          emptyProducts: t("catalog.empty"),
          shopCta: t("content.shopCta"),
        }}
      />
      <TraceForm className="glass-strong" />
    </>
  )
}
