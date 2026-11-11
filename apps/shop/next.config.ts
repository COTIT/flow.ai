import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

import { securityHeaders } from "@genesy/security/headers"

// shortcut: localhost default for dev; every deploy must set API_URL.
const apiUrl = process.env.API_URL ?? "http://localhost:8080/v1"

const nextConfig: NextConfig = {
  output: process.env.NEXT_OUTPUT === "standalone" ? "standalone" : undefined,
  // The full-stack e2e harness builds to its own folder (packages/e2e/STACK.md).
  distDir: process.env.NEXT_DIST_DIR ?? ".next",
  transpilePackages: [
    "@genesy/ui",
    "@genesy/api-client",
    "@genesy/i18n",
    "@genesy/security",
  ],
  headers: async () => [
    { source: "/:path*", headers: securityHeaders },
    // Preview links carry a bearer token in the path: never leak it in Referer, never index it.
    {
      source: "/preview/:path*",
      headers: [
        { key: "Referrer-Policy", value: "no-referrer" },
        { key: "X-Robots-Tag", value: "noindex, nofollow" },
      ],
    },
  ],
  // The browser calls same-origin /v1 and Next forwards it: the backend sends no CORS headers.
  rewrites: async () => [
    { source: "/v1/:path*", destination: `${apiUrl}/:path*` },
  ],
}

export default createNextIntlPlugin()(nextConfig)
