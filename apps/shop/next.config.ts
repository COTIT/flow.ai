import type { NextConfig } from "next"

const nextConfig: NextConfig = {
  transpilePackages: ["@genesy/ui", "@genesy/api-client"],
}

export default nextConfig
