import { ogImage, ogSize } from "@/lib/og-image"

// Same words as messages/en.json landing.ogAlt (a route file needs a plain value here).
export const alt =
  "Genesy: from the hive to your kitchen, and the farmer paid along the way"
export const size = ogSize
export const contentType = "image/png"

export default function Image() {
  return ogImage("en")
}
