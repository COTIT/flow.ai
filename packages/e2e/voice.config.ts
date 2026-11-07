import { defineConfig } from "@playwright/test"
import { projectsFor } from "./src/projects"

// Focused production-build run for the voice page and Office's existing regressions.
export default defineConfig({
  projects: projectsFor("office", {
    testDir: "tests/office",
    baseURL: "http://127.0.0.1:3261",
  }),
  use: { trace: "on-first-retry" },
  webServer: {
    command:
      "pnpm --filter @genesy/office exec next start --port 3261 --hostname 127.0.0.1",
    url: "http://127.0.0.1:3261/login",
    reuseExistingServer: false,
    env: { API_URL: "http://127.0.0.1:9/v1", LANGSMITH_TRACING: "false" },
  },
})
