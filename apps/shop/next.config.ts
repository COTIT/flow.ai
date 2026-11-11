import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

import { securityHeaders } from "@genesy/security/headers"

// shortcut: localhost default for dev; every deploy must set API_URL.
const apiUrl = process.env.API_URL ?? "http://localhost:8080/v1"

const nextConfig: NextConfig = {
  // Keep internal rewrites on the server's original origin, including loopback/container hosts.
  skipProxyUrlNormalize: true,
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
    // Everything except /preview: one rule per path, so no header is sent twice.
    { source: "/:path((?!preview(?:/|$)).*)", headers: securityHeaders },
    // Preview links carry a bearer token in the path: no Referer, no index, no cache.
    {
      source: "/preview/:path*",
      headers: [
        ...securityHeaders.filter((h) => h.key !== "Referrer-Policy"),
        { key: "Referrer-Policy", value: "no-referrer" },
        { key: "X-Robots-Tag", value: "noindex, nofollow" },
        { key: "Cache-Control", value: "no-store" },
      ],
    },
  ],
  // The browser calls same-origin /v1 and Next forwards it: the backend sends no CORS headers.
  rewrites: async () => [
    { source: "/v1/:path*", destination: `${apiUrl}/:path*` },
  ],
}

export default createNextIntlPlugin()(nextConfig)
