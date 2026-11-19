import type { Metadata } from "next"
import { cache } from "react"
import Link from "@/components/shop-link"
import { getLocale, getTranslations } from "next-intl/server"

import { Button, buttonVariants } from "@verzo/ui/components/button"
import { EmptyState } from "@verzo/ui/components/empty-state"
import { cn } from "@verzo/ui/lib/utils"

import { CategoryChips } from "@/components/category-chips"
import { ProductCard } from "@/components/product-card"
import { card, h2, prose, warm } from "@/lib/look"
import { listCategories, listProducts } from "@/lib/storefront"
import { cataloguePath, pageMetadata } from "@/lib/seo"
import { getShop, getShopBase } from "@/lib/shop-scope"
import { shopNoindex } from "@/lib/site-origin"

type QueryValue = string | string[]
type Props = {
  searchParams: Promise<{ page?: QueryValue; category?: QueryValue }>
}

const readCatalogue = cache(async (page: number, category?: QueryValue) => {
  const categories = await listCategories()
  const chosen = category
    ? categories.find((c) => c.id === category)
    : undefined
  const unknown = Boolean(category) && !chosen
  const products = unknown ? null : await listProducts(page, chosen?.id)
  return { categories, chosen, unknown, products }
})

const pageNumber = (page?: QueryValue) =>
  Math.max(1, Number.parseInt(page?.toString() ?? "", 10) || 1)

export async function generateMetadata({
  searchParams,
}: Props): Promise<Metadata> {
  const { category, page } = await searchParams
  const t = await getTranslations()
  if (await getShopBase()) {
    const chosen = category
      ? (await listCategories()).find((c) => c.id === category)
      : undefined
    return { title: chosen?.name ?? t("catalog.title") }
  }

  const { chosen, products } = await readCatalogue(pageNumber(page), category)
  const title = chosen?.name ?? t("catalog.title")
  const validPage =
    page === undefined ||
    (typeof page === "string" &&
      /^\d+$/.test(page) &&
      Number.isSafeInteger(Number(page)) &&
      Number(page) > 0)

  if (
    !validPage ||
    !products?.data.length ||
    products.meta.current_page > products.meta.last_page
  )
    return { title, robots: { index: false, follow: !shopNoindex() } }

  return pageMetadata(cataloguePath(products.meta.current_page, chosen?.id), {
    title,
    description: chosen?.description ?? t("app.tagline"),
  })
}

const href = (page: number, category?: string) =>
  `/shop?${new URLSearchParams({ ...(category && { category }), page: String(page) })}`

/** The catalogue: one category or all, its chips, the product grid and its pager. */
export default async function ShopPage({ searchParams }: Props) {
  const params = await searchParams
  const [t, locale, { categories, chosen, unknown, products }] =
    await Promise.all([
      getTranslations(),
      getLocale(),
      readCatalogue(pageNumber(params.page), params.category),
    ])
  const shop = await getShop()
  const labels = {
    inStock: t("product.inStock"),
    low: t("product.low"),
    soldOut: t("product.soldOut"),
    imageSoon: t("product.imageSoon"),
  }
  const allLink = (
    <Link href="/shop" data-slot="button" className={buttonVariants()}>
      {t("catalog.seeAll")}
    </Link>
  )

  return (
    <>
      <section
        className={cn(
          card,
          warm,
          "afm-intro afm-catalog-intro gap-4 md:gap-5 md:p-8"
        )}
      >
        <h1 className="text-[28px] leading-[1.15] font-medium md:text-[36px]">
          {chosen?.name ?? t("catalog.title")}
        </h1>
        <p className={cn(prose, "text-muted-foreground")}>
          {chosen?.description ?? shop?.name ?? t("app.tagline")}
        </p>
        {categories.length ? (
          <CategoryChips
            categories={categories}
            current={chosen?.id}
            label={t("catalog.categories")}
            all={t("catalog.allCategories")}
          />
        ) : null}
      </section>

      {unknown ? (
        <div className={card}>
          <EmptyState
            title={t("catalog.unknownCategory")}
            description={t("catalog.unknownCategoryWhy")}
            action={allLink}
          />
        </div>
      ) : products ? (
        <section aria-labelledby="products" className="flex flex-col gap-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <h2 id="products" className={h2}>
              {chosen?.name ?? t("catalog.all")}
            </h2>
            {products.data.length ? (
              <p className="text-[13px] text-muted-foreground">
                {t("catalog.count", {
                  shown: products.data.length,
                  total: products.meta.total,
                })}
              </p>
            ) : null}
          </div>

          {products.data.length === 0 ? (
            <div className={card}>
              {products.meta.total > 0 ? (
                <EmptyState
                  title={t("catalog.pastEnd")}
                  description={t("catalog.pastEndWhy")}
                  action={
                    <Link
                      href={href(1, chosen?.id)}
                      data-slot="button"
                      className={buttonVariants()}
                    >
                      {t("catalog.firstPage")}
                    </Link>
                  }
                />
              ) : chosen ? (
                <EmptyState
                  title={t("catalog.emptyCategory")}
                  description={t("catalog.emptyCategoryWhy")}
                  action={allLink}
                />
              ) : (
                <EmptyState
                  title={t("catalog.empty")}
                  description={t("catalog.emptyWhy")}
                />
              )}
            </div>
          ) : (
            <ul
              aria-labelledby="products"
              className="afm-product-grid grid grid-cols-2 gap-3 md:grid-cols-3 md:gap-5 lg:grid-cols-4"
            >
              {products.data.map((p) => (
                <li key={p.id} className="flex min-w-0">
                  <ProductCard product={p} locale={locale} labels={labels} />
                </li>
              ))}
            </ul>
          )}

          {products.meta.last_page > 1 ? (
            <Pager
              page={products.meta.current_page}
              last={products.meta.last_page}
              category={chosen?.id}
              t={t}
            />
          ) : null}
        </section>
      ) : null}
    </>
  )
}

function Pager({
  page,
  last,
  category,
  t,
}: {
  page: number
  last: number
  category?: string
  t: Awaited<ReturnType<typeof getTranslations>>
}) {
  const step = (to: number, label: string) =>
    to >= 1 && to <= last ? (
      <Link
        href={href(to, category)}
        data-slot="button"
        className={buttonVariants({ variant: "outline" })}
      >
        {label}
      </Link>
    ) : (
      <Button variant="outline" disabled>
        {label}
      </Button>
    )
  return (
    <nav
      className="afm-pager flex items-center justify-between gap-3"
      aria-label={t("catalog.pages")}
    >
      {step(page - 1, t("catalog.previous"))}
      <span className="text-[13px] text-muted-foreground">
        {t("catalog.page", { page, last })}
      </span>
      {step(page + 1, t("catalog.next"))}
    </nav>
  )
}
