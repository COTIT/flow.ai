import { shopNoindex } from "./site-origin"

export const SITEMAP_PAGE_SIZE = 100
// One index entry is reserved for the public pages sitemap.
export const MAX_PRODUCT_SHARDS = 49_999

const escapeXml = (value: string) =>
  value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;")

export const xmlLocation = (url: string) => `<loc>${escapeXml(url)}</loc>`

export const xmlImage = (url: string) =>
  `<image:image><image:loc>${escapeXml(url)}</image:loc></image:image>`

export function unavailableResponse() {
  return new Response(null, {
    status: 503,
    headers: { "Cache-Control": "no-store", "Retry-After": "60" },
  })
}

export function missingResponse() {
  return new Response(null, {
    status: 404,
    headers: { "Cache-Control": "no-store" },
  })
}

export async function sitemapResponse(
  render: () => Promise<string>
): Promise<Response> {
  if (shopNoindex()) return missingResponse()
  try {
    return new Response(
      `<?xml version="1.0" encoding="UTF-8"?>\n${await render()}\n`,
      {
        headers: {
          "Content-Type": "application/xml; charset=utf-8",
          "Cache-Control": "public, max-age=0, s-maxage=60",
        },
      }
    )
  } catch {
    return unavailableResponse()
  }
}
