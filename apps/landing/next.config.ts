import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

import { securityHeaders } from "@genesy/security/headers"

const nextConfig: NextConfig = {
  output: process.env.NEXT_OUTPUT === "standalone" ? "standalone" : undefined,
  // The full-stack e2e harness builds to its own folder (packages/e2e/STACK.md).
  distDir: process.env.NEXT_DIST_DIR ?? ".next",
  transpilePackages: ["@genesy/ui", "@genesy/i18n", "@genesy/security"],
  headers: async () => [{ source: "/:path*", headers: securityHeaders }],
}

export default createNextIntlPlugin()(nextConfig)
