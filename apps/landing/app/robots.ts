import type { MetadataRoute } from "next"

import { noindex, robotsRules } from "@/lib/seo"

// Read per request: the same image runs as production (indexed) and staging (NOINDEX=1).
export const dynamic = "force-dynamic"

export default function robots(): MetadataRoute.Robots {
  return robotsRules(noindex())
}
