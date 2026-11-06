import type { Metadata } from "next"
import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { cn } from "@genesy/ui/lib/utils"

import { CategoryChips } from "@/components/category-chips"
import { ProductCard } from "@/components/product-card"
import { card, h2, prose, warm } from "@/lib/look"
import { listCategories, listProducts } from "@/lib/storefront"

type Props = {
  searchParams: Promise<{ page?: string; category?: string }>
}

export async function generateMetadata({
  searchParams,
}: Props): Promise<Metadata> {
  const { category } = await searchParams
  const chosen = category
    ? (await listCategories()).find((c) => c.id === category)
    : undefined
  return { title: chosen?.name ?? (await getTranslations("catalog"))("title") }
}

const href = (page: number, category?: string) =>
  `/shop?${new URLSearchParams({ ...(category && { category }), page: String(page) })}`

/** The catalogue: one category or all, its chips, the product grid and its pager. */
export default async function ShopPage({ searchParams }: Props) {
  const params = await searchParams
  const page = Math.max(1, Number.parseInt(params.page ?? "", 10) || 1)
  const [t, locale, categories] = await Promise.all([
    getTranslations(),
    getLocale(),
    listCategories(),
  ])
  const chosen = params.category
    ? categories.find((c) => c.id === params.category)
    : undefined
  const unknown = Boolean(params.category) && !chosen
  const products = unknown ? null : await listProducts(page, chosen?.id)
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
      <section className={cn(card, warm, "gap-4 md:gap-5 md:p-8")}>
        <h1 className="text-[28px] leading-[1.15] font-medium md:text-[36px]">
          {chosen?.name ?? t("catalog.title")}
        </h1>
        <p className={cn(prose, "text-muted-foreground")}>
          {chosen?.description ?? t("app.tagline")}
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
              className="grid grid-cols-2 gap-3 md:grid-cols-3 md:gap-5 lg:grid-cols-4"
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
      className="flex items-center justify-between gap-3"
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
