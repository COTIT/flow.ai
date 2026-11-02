import { ogImage, ogSize } from "@/lib/og-image"

// Same words as messages/sw.json landing.ogAlt (a route file needs a plain value here).
export const alt =
  "Genesy: kutoka mzingani hadi jikoni kwako, na mkulima analipwa njiani"
export const size = ogSize
export const contentType = "image/png"

export default function Image() {
  return ogImage("sw")
}
