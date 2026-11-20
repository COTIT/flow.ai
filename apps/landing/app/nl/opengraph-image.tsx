import { ogImage, ogSize } from "@/lib/og-image"

// Same words as messages/nl.json landing.ogAlt (a route file needs a plain value here).
export const alt = "Verzo. Zet uw intenties om in acties."
export const size = ogSize
export const contentType = "image/png"

export default function Image() {
  return ogImage("nl")
}
