import { ogImage, ogSize } from "@/lib/og-image"

// Same words as messages/nl.json landing.ogAlt (a route file needs a plain value here).
export const alt =
  "Genesy: van de korf tot uw keuken, en de boer onderweg betaald"
export const size = ogSize
export const contentType = "image/png"

export default function Image() {
  return ogImage("nl")
}
