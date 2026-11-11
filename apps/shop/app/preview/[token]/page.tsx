import type { Metadata } from "next"
import { getLocale, getTranslations } from "next-intl/server"

import { buttonVariants } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { StorefrontPage } from "@genesy/ui/components/storefront-page"
import { card } from "@/lib/look"
import { getPreviewDesign } from "@/lib/storefront-design"

// Bearer link: never indexed; next.config adds no-referrer, noindex and no-store headers too.
export const metadata: Metadata = { robots: { index: false, follow: false } }
// Every request re-checks the link: no cached render (Cache-Control no-store).
export const dynamic = "force-dynamic"

/**
 * Unpublished draft behind an expiring preview link, inside the real Shop chrome. The body is
 * inert (no product links, forms or cart). Any dead link shows the same expired page.
 */
export default async function PreviewPage({
  params,
}: {
  params: Promise<{ token: string }>
}) {
  const { token } = await params
  const [design, t, locale] = await Promise.all([
    getPreviewDesign(token),
    getTranslations(),
    getLocale(),
  ])
  if (!design) {
    const flow = process.env.FLOW_URL
    return (
      <div className={card}>
        <EmptyState
          title={
            <span role="heading" aria-level={1}>
              {t("preview.expiredTitle")}
            </span>
          }
          description={t("preview.expiredBody")}
          action={
            flow ? (
              <a
                href={flow}
                data-slot="button"
                className={buttonVariants({ variant: "outline", size: "sm" })}
              >
                {t("preview.backToFlow")}
              </a>
            ) : undefined
          }
        />
      </div>
    )
  }
  const expires = new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: "UTC",
    timeZoneName: "short",
  }).format(new Date(design.expires_at))
  return (
    <>
      <Notice variant="info" title={t("preview.banner")} live={false}>
        {t("preview.revision", { id: design.revision_id.slice(0, 8) })} ·{" "}
        <time dateTime={design.expires_at}>
          {t("preview.expires", { time: expires })}
        </time>
      </Notice>
      <StorefrontPage
        document={design.document}
        collections={design.collections}
        locale={locale}
        preview
        headings="page"
        labels={{
          inStock: t("product.inStock"),
          low: t("product.low"),
          soldOut: t("product.soldOut"),
          imageSoon: t("product.imageSoon"),
          emptyProducts: t("catalog.empty"),
          shopCta: t("content.shopCta"),
        }}
      />
    </>
  )
}
