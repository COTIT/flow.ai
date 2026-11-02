import type { MetadataRoute } from "next"

import { locales } from "@genesy/i18n/locales"
import { pageUrl } from "@/lib/seo"

export default function sitemap(): MetadataRoute.Sitemap {
  return locales.map((locale) => ({
    url: pageUrl(locale),
    changeFrequency: "monthly",
    alternates: {
      languages: Object.fromEntries(locales.map((l) => [l, pageUrl(l)])),
    },
  }))
}
