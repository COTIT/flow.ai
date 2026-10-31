import type { NextConfig } from "next"
import createNextIntlPlugin from "next-intl/plugin"

// shortcut: localhost default for dev; every deploy must set API_URL.
const apiUrl = process.env.API_URL ?? "http://localhost:8080/v1"

const nextConfig: NextConfig = {
  transpilePackages: ["@genesy/ui", "@genesy/api-client", "@genesy/i18n"],
  // The browser calls same-origin /v1 and Next forwards it: the backend sends no CORS headers.
  rewrites: async () => [
    { source: "/v1/:path*", destination: `${apiUrl}/:path*` },
  ],
}

export default createNextIntlPlugin()(nextConfig)
