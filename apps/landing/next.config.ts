import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

import { securityHeaders } from "@genesy/security/headers"

const nextConfig: NextConfig = {
  output: process.env.NEXT_OUTPUT === "standalone" ? "standalone" : undefined,
  transpilePackages: ["@genesy/ui", "@genesy/i18n", "@genesy/security"],
  headers: async () => [{ source: "/:path*", headers: securityHeaders }],
}

export default createNextIntlPlugin()(nextConfig)
