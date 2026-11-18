import type { Metadata } from "next"
import { getTranslations } from "next-intl/server"

import { shopNoindex, siteUrl } from "./site-origin"

type PageDetails = {
  title: string
  description?: string
  type?: "website" | "article"
  image?: string
}

export async function pageMetadata(
  path: string,
  { title, description, type = "website", image }: PageDetails
): Promise<Metadata> {
  const siteName = (await getTranslations("app"))("title")
  const url = siteUrl(path)
  const images = image ? [siteUrl(image)] : undefined
  return {
    title,
    description,
    alternates: { canonical: url },
    openGraph: { siteName, title, description, type, url, images },
    twitter: {
      card: image ? "summary_large_image" : "summary",
      title,
      description,
      images,
    },
    ...(shopNoindex() && { robots: { index: false, follow: false } }),
  }
}

export function cataloguePath(page: number, category?: string): string {
  const params = new URLSearchParams()
  if (category) params.set("category", category)
  if (page > 1) params.set("page", String(page))
  return params.size ? `/shop?${params}` : "/shop"
}

export function websiteSchema(name: string) {
  const url = siteUrl("/")
  const organization = `${url}#organization`
  return {
    "@context": "https://schema.org",
    "@graph": [
      {
        "@type": "Organization",
        "@id": organization,
        name,
        url,
      },
      {
        "@type": "WebSite",
        "@id": `${url}#website`,
        name,
        url,
        publisher: { "@id": organization },
      },
    ],
  }
}
