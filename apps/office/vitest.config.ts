import { createRequire } from "node:module"
import { fileURLToPath } from "node:url"
import { defineConfig } from "vitest/config"

const require = createRequire(import.meta.url)

export default defineConfig({
  // tsconfig keeps JSX for Next ("preserve"); tests need it compiled.
  oxc: { jsx: { runtime: "automatic" } },
  resolve: {
    alias: {
      "@/": fileURLToPath(new URL("./", import.meta.url)),
      // Next swaps this guard in at build time; tests run on the server side anyway.
      "server-only": require.resolve("next/dist/compiled/server-only/empty.js"),
    },
  },
})
