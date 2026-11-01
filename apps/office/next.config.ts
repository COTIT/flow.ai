import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

import { securityHeaders } from "@genesy/security/headers"

// shortcut: localhost default for dev; every deploy must set API_URL.
const apiUrl = process.env.API_URL ?? "http://localhost:8080/v1"

const nextConfig: NextConfig = {
  output: process.env.NEXT_OUTPUT === "standalone" ? "standalone" : undefined,
  transpilePackages: [
    "@genesy/ui",
    "@genesy/api-client",
    "@genesy/i18n",
    "@genesy/security",
  ],
  headers: async () => [{ source: "/:path*", headers: securityHeaders }],
  // The browser calls same-origin /v1 and Next forwards it: the backend sends no CORS headers.
  rewrites: async () => [
    { source: "/v1/:path*", destination: `${apiUrl}/:path*` },
  ],
}

export default createNextIntlPlugin()(nextConfig)
