import type { Metadata } from "next"
import { redirect } from "next/navigation"
import { getLocale, getTranslations } from "next-intl/server"

import { StorefrontPage } from "@genesy/ui/components/storefront-page"
import LegacyHomePage from "@/components/legacy-home"
import { ShopJsonLd } from "@/components/shop-json-ld"
import { TraceForm } from "@/components/trace-form"
import { PHOTOS } from "@/lib/photos"
import { pageMetadata, websiteSchema } from "@/lib/seo"
import { getPublishedDesign } from "@/lib/storefront-design"
import { getShopBase } from "@/lib/shop-scope"
import { shopPath } from "@/lib/shop-path"
import ShopPage from "./shop/page"

export async function generateMetadata(): Promise<Metadata> {
  if (await getShopBase()) return {}
  const t = await getTranslations("app")
  const title = t("title")
  return {
    ...(await pageMetadata("/", {
      title,
      description: t("tagline"),
      image: PHOTOS.landscape.src,
    })),
    title: { absolute: title },
  }
}

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
  const schema = base ? null : (
    <ShopJsonLd data={websiteSchema((await getTranslations("app"))("title"))} />
  )
  if (!design)
    return base ? (
      <ShopPage searchParams={Promise.resolve({})} />
    ) : (
      <>
        {schema}
        <LegacyHomePage />
      </>
    )

  const [t, locale] = await Promise.all([getTranslations(), getLocale()])
  return (
    <>
      {schema}
      <div data-shop-document>
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
      </div>
      <TraceForm className="glass-strong" />
    </>
  )
}
