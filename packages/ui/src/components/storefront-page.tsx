import type {
  StorefrontDesignCollection,
  StorefrontDesignDocument,
} from "@genesy/api-client/model"
import { canRenderStorefrontPage } from "../lib/storefront-design"
import { formatMoney } from "../lib/money"
import { shownPrice } from "../lib/storefront-price"
import { cn } from "../lib/utils"
import { Badge } from "./badge"
import { buttonVariants } from "./button"

/** Same presentation serves public Shop and authenticated, inert Office previews. */
export type StorefrontPageProps = {
  document: StorefrontDesignDocument
  collections: StorefrontDesignCollection[]
  locale: string
  labels: {
    inStock: string
    low: string
    soldOut: string
    imageSoon: string
    emptyProducts: string
    shopCta: string
  }
  preview?: boolean
  basePath?: string
  /**
   * Heading levels: "page" starts at h1 (then h2), "nested" puts every section at h3.
   * Separate from `preview`, which only makes the page inert: a preview embedded in a host
   * page sits under its headings, but a framed preview is its own document.
   * Defaults to "nested" for previews, "page" otherwise.
   */
  headings?: "page" | "nested"
}

const mobileColumns: Record<number, string> = {
  1: "grid-cols-1",
  2: "grid-cols-2",
}
const tabletColumns: Record<number, string> = {
  1: "md:grid-cols-1",
  2: "md:grid-cols-2",
  3: "md:grid-cols-3",
}
const desktopColumns: Record<number, string> = {
  1: "lg:grid-cols-1",
  2: "lg:grid-cols-2",
  3: "lg:grid-cols-3",
  4: "lg:grid-cols-4",
}
const stock = {
  in_stock: { tone: "teal", label: "inStock" },
  low: { tone: "gold", label: "low" },
  sold_out: { tone: "slate", label: "soldOut" },
} as const

/** Fixed original blocks; no runtime scripts, arbitrary styles or data fetching. */
export function StorefrontPage({
  document,
  collections,
  locale,
  labels,
  preview = false,
  basePath = "",
  headings = preview ? "nested" : "page",
}: StorefrontPageProps) {
  if (!canRenderStorefrontPage(document, collections)) return null
  return (
    <div className="flex min-w-0 flex-col gap-5 md:gap-7">
      {document.sections.map((section, index) => {
        // Nested: Office's embedded preview sits under the page h1 and its own h2.
        const Heading = headings === "nested" ? "h3" : index === 0 ? "h1" : "h2"
        if (section.type === "hero") {
          const centered = section.props.layout === "centered"
          return (
            <section
              key={section.id}
              aria-labelledby={`design-${section.id}`}
              className={cn(
                "flex min-w-0 flex-col gap-5 rounded-[20px] glass p-6 md:p-12",
                document.theme === "forest"
                  ? "bg-(--tint-success)"
                  : "bg-(--glass-subtle)",
                centered
                  ? "items-center text-center"
                  : "md:grid md:grid-cols-2 md:items-center md:gap-10"
              )}
            >
              <Heading
                id={`design-${section.id}`}
                className="max-w-[20ch] text-3xl leading-tight font-medium break-words md:text-5xl"
              >
                {section.props.heading}
              </Heading>
              <div
                className={cn(
                  "flex min-w-0 flex-col items-start gap-5",
                  centered && "items-center"
                )}
              >
                {section.props.body ? (
                  <p className="max-w-[65ch] leading-relaxed break-words text-muted-foreground">
                    {section.props.body}
                  </p>
                ) : null}
                {preview ? (
                  <span
                    className={cn(buttonVariants({ size: "lg" }), "min-h-11")}
                  >
                    {labels.shopCta}
                  </span>
                ) : (
                  <a
                    href={`${basePath}/shop`}
                    className={cn(buttonVariants({ size: "lg" }), "min-h-11")}
                  >
                    {labels.shopCta}
                  </a>
                )}
              </div>
            </section>
          )
        }
        const products = collections.find(
          (collection) => collection.section_id === section.id
        )!.products
        const columns = section.props.columns ?? {
          mobile: 1,
          tablet: 2,
          desktop: 3,
        }
        const ProductHeading =
          Heading === "h1" ? "h2" : Heading === "h2" ? "h3" : "h4"
        return (
          <section
            key={section.id}
            aria-labelledby={`design-${section.id}`}
            className="flex min-w-0 flex-col gap-4"
          >
            <Heading
              id={`design-${section.id}`}
              className="text-[22px] leading-snug font-medium break-words"
            >
              {section.props.heading}
            </Heading>
            {products.length ? (
              <ul
                className={cn(
                  "grid gap-3 md:gap-5",
                  mobileColumns[columns.mobile],
                  tabletColumns[columns.tablet],
                  desktopColumns[columns.desktop]
                )}
              >
                {products.map((product) => {
                  const tone = stock[product.stock_status as keyof typeof stock]
                  const Card = preview ? "div" : "a"
                  return (
                    <li key={product.id} className="flex min-w-0">
                      <Card
                        {...(!preview && {
                          href: `${basePath}/products/${product.id}`,
                        })}
                        className="flex w-full min-w-0 flex-col overflow-hidden rounded-[18px] glass outline-none focus-visible:ring-3 focus-visible:ring-ring/60"
                      >
                        {product.images[0] ? (
                          <img
                            src={product.images[0].url}
                            alt=""
                            width={640}
                            height={480}
                            loading="lazy"
                            referrerPolicy="no-referrer"
                            className="aspect-[4/3] w-full object-cover"
                          />
                        ) : (
                          <div
                            aria-hidden="true"
                            className="flex aspect-[4/3] items-center justify-center bg-muted p-4 text-sm text-muted-foreground"
                          >
                            {labels.imageSoon}
                          </div>
                        )}
                        <div className="flex min-w-0 flex-1 flex-col gap-2 p-4">
                          <ProductHeading className="font-semibold break-words">
                            {product.name}
                          </ProductHeading>
                          {product.short_description ? (
                            <p className="text-sm break-words text-muted-foreground">
                              {product.short_description}
                            </p>
                          ) : null}
                          <div className="mt-auto flex flex-wrap items-center justify-between gap-3 pt-2">
                            <span className="font-semibold tabular-nums">
                              {formatMoney(
                                shownPrice(product),
                                product.currency_decimals,
                                product.currency,
                                locale
                              )}
                            </span>
                            {tone ? (
                              <Badge variant={tone.tone}>
                                {labels[tone.label]}
                              </Badge>
                            ) : null}
                          </div>
                        </div>
                      </Card>
                    </li>
                  )
                })}
              </ul>
            ) : (
              <p className="text-muted-foreground">{labels.emptyProducts}</p>
            )}
          </section>
        )
      })}
    </div>
  )
}
