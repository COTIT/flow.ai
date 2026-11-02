import type { Metadata, MetadataRoute } from "next"

import type { Locale } from "@genesy/i18n/locales"

/** The public address. Canonical and hreflang links always name it, also on staging (which is noindex). */
export const SITE = "https://genesy.app"

const PATH: Record<Locale, string> = { en: "", nl: "/nl", sw: "/sw" }
export const pageUrl = (locale: Locale) => `${SITE}${PATH[locale]}`

/** Staging sets NOINDEX=1: the copy stays out of search engines. */
export const noindex = () => process.env.NOINDEX === "1"

export function pageMetadata(
  locale: Locale,
  { title, description }: { title: string; description: string },
  hidden: boolean
): Metadata {
  return {
    title,
    description,
    alternates: {
      canonical: pageUrl(locale),
      languages: {
        en: pageUrl("en"),
        nl: pageUrl("nl"),
        sw: pageUrl("sw"),
        "x-default": pageUrl("en"),
      },
    },
    openGraph: {
      type: "website",
      siteName: "Genesy",
      title,
      description,
      url: pageUrl(locale),
      locale,
    },
    twitter: { card: "summary_large_image", title, description },
    ...(hidden ? { robots: { index: false, follow: false } } : {}),
  }
}

export function robotsRules(hidden: boolean): MetadataRoute.Robots {
  return hidden
    ? { rules: { userAgent: "*", disallow: "/" } }
    : { rules: { userAgent: "*", allow: "/" }, sitemap: `${SITE}/sitemap.xml` }
}
