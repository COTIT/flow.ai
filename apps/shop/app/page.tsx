import Link from "next/link"
import { getLocale, getTranslations } from "next-intl/server"
import { QrCodeIcon } from "lucide-react"

import { Button, buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { cn } from "@genesy/ui/lib/utils"

import { Headline } from "@/components/motion"
import { ProductCard } from "@/components/product-card"
import { card, goldChip, h2, tag, tile } from "@/lib/look"
import { listProducts } from "@/lib/storefront"

/** Mock `ShopCatalogue`: hero card, then the product grid with its pager. */
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
  const { current_page, last_page, total } = products.meta
  const labels = {
    inStock: t("product.inStock"),
    soldOut: t("product.soldOut"),
    imageSoon: t("product.imageSoon"),
  }

  return (
    <>
      <section
        className={cn(
          card,
          "flex-row flex-wrap items-center gap-5 p-8 md:flex-row md:gap-5 md:p-8"
        )}
      >
        <div className="flex min-w-0 flex-[1_1_420px] flex-col gap-3">
          <p className={cn(tag, "text-(--tone-warning)")}>{t("catalog.tag")}</p>
          <h1 className="max-w-[640px] text-[34px] leading-[1.2] font-medium tracking-[0.04em] uppercase">
            <Headline text={t("catalog.headline")} />
          </h1>
          <p className="max-w-[560px] text-muted-foreground">
            {t("app.tagline")}
          </p>
        </div>
        <div className={cn(tile, "flex flex-[0_1_320px] items-center gap-3.5")}>
          <span className={cn(goldChip, "size-[52px] rounded-[16px]")}>
            <QrCodeIcon aria-hidden className="size-[22px]" />
          </span>
          <div>
            <p className="font-semibold">{t("catalog.scanTitle")}</p>
            <p className="text-[13px] text-muted-foreground">
              {t("catalog.scanBody")}
            </p>
          </div>
        </div>
      </section>

      <section aria-labelledby="all-products" className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <h2 id="all-products" className={h2}>
            {t("catalog.all")}
          </h2>
          {products.data.length ? (
            <p className="text-[13px] text-muted-foreground">
              {t("catalog.count", { shown: products.data.length, total })}
            </p>
          ) : null}
        </div>

        {products.data.length === 0 ? (
          <div className={card}>
            {total > 0 ? (
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
            )}
          </div>
        ) : (
          <ul
            aria-labelledby="all-products"
            className="grid grid-cols-2 gap-3 md:grid-cols-3 md:gap-5"
          >
            {products.data.map((p) => (
              <li key={p.id} className="flex min-w-0">
                <ProductCard product={p} locale={locale} labels={labels} />
              </li>
            ))}
          </ul>
        )}

        {last_page > 1 ? (
          <nav
            className="flex items-center justify-between gap-3"
            aria-label={t("catalog.pages")}
          >
            {current_page > 1 ? (
              <Link
                href={`/?page=${current_page - 1}`}
                className={buttonVariants({ variant: "outline" })}
              >
                {t("catalog.previous")}
              </Link>
            ) : (
              <Button variant="outline" disabled>
                {t("catalog.previous")}
              </Button>
            )}
            <span className="text-[13px] text-muted-foreground">
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
              <Button variant="outline" disabled>
                {t("catalog.next")}
              </Button>
            )}
          </nav>
        ) : null}
      </section>
    </>
  )
}
