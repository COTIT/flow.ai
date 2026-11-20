import { defineConfig } from "orval"

// Generates typed axios calls + TanStack Query hooks from the backend's OpenAPI file.
// Regenerate: `pnpm --filter @verzo/api-client generate`.
export default defineConfig({
  verzo: {
    input: "../../apps/api/openapi.json",
    output: {
      mode: "tags-split",
      target: "src/generated/endpoints.ts",
      schemas: "src/generated/model",
      client: "react-query",
      httpClient: "axios",
      clean: true,
      override: {
        mutator: { path: "./src/mutator.ts", name: "apiRequest" },
      },
    },
  },
})
