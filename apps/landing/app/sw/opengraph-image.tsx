import { ogImage, ogSize } from "@/lib/og-image"

// Same words as messages/sw.json landing.ogAlt (a route file needs a plain value here).
export const alt = "Verzo. Geuza nia zako kuwa vitendo."
export const size = ogSize
export const contentType = "image/png"

export default function Image() {
  return ogImage("sw")
}
