import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"

import { ProductCard } from "@/components/product-card"
import { listProducts } from "@/lib/storefront"

export default async function CatalogPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const page = Math.max(
    1,
    Number.parseInt((await searchParams).page ?? "", 10) || 1
  )
  const [t, locale, products] = await Promise.all([
    getTranslations(),
    getLocale(),
    listProducts(page),
  ])
  const { current_page, last_page } = products.meta
  const labels = {
    inStock: t("product.inStock"),
    soldOut: t("product.soldOut"),
    imageSoon: t("product.imageSoon"),
  }

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-col gap-2">
        <h1 className="heading-1">{t("catalog.title")}</h1>
        <p className="max-w-2xl text-muted-foreground">{t("app.tagline")}</p>
      </div>

      {products.data.length === 0 ? (
        products.meta.total > 0 ? (
          <EmptyState
            title={t("catalog.pastEnd")}
            description={t("catalog.pastEndWhy")}
            action={
              <Link href="/" className={buttonVariants()}>
                {t("catalog.firstPage")}
              </Link>
            }
          />
        ) : (
          <EmptyState
            title={t("catalog.empty")}
            description={t("catalog.emptyWhy")}
          />
        )
      ) : (
        <ul className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {products.data.map((p) => (
            <li key={p.id} className="flex">
              <ProductCard product={p} locale={locale} labels={labels} />
            </li>
          ))}
        </ul>
      )}

      {last_page > 1 ? (
        <nav
          className="flex items-center justify-between gap-2"
          aria-label={t("catalog.title")}
        >
          {current_page > 1 ? (
            <Link
              href={`/?page=${current_page - 1}`}
              className={buttonVariants({ variant: "outline" })}
            >
              {t("catalog.previous")}
            </Link>
          ) : (
            <span />
          )}
          <span className="text-sm text-muted-foreground">
            {t("catalog.page", { page: current_page, last: last_page })}
          </span>
          {current_page < last_page ? (
            <Link
              href={`/?page=${current_page + 1}`}
              className={buttonVariants({ variant: "outline" })}
            >
              {t("catalog.next")}
            </Link>
          ) : (
            <span />
          )}
        </nav>
      ) : null}
    </div>
  )
}
