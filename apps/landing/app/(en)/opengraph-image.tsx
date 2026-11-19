import { ogImage, ogSize } from "@/lib/og-image"

// Same words as messages/en.json landing.ogAlt (a route file needs a plain value here).
export const alt = "Verzo. Turn your intentions into actions."
export const size = ogSize
export const contentType = "image/png"

export default function Image() {
  return ogImage("en")
}
