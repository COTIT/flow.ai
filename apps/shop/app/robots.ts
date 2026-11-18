import type { MetadataRoute } from "next"

import { shopNoindex, siteUrl } from "@/lib/site-origin"

export const dynamic = "force-dynamic"

export default function robots(): MetadataRoute.Robots {
  if (shopNoindex()) return { rules: { userAgent: "*", disallow: "/" } }
  return {
    rules: {
      userAgent: "*",
      allow: "/",
      disallow: "/v1/",
    },
    sitemap: siteUrl("/sitemap.xml"),
  }
}
