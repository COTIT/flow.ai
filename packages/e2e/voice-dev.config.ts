import { defineConfig } from "@playwright/test"
import { projectsFor } from "./src/projects"

export default defineConfig({
  // Strict Mode repeats effect setup/cleanup in development; production tests cannot exercise it.
  grep: /secure handoff closes conversation before collecting credentials/,
  timeout: 60000,
  projects: projectsFor("office", {
    testDir: "tests/office",
    baseURL: "http://127.0.0.1:3262",
  }),
  webServer: {
    command:
      "pnpm --filter @genesy/office exec next dev --webpack --port 3262 --hostname 127.0.0.1",
    url: "http://127.0.0.1:3262/login",
    timeout: 150000,
    reuseExistingServer: false,
    env: {
      NEXT_DIST_DIR: ".next-voice-dev",
      API_URL: "http://127.0.0.1:9/v1",
      LANGSMITH_TRACING: "false",
    },
  },
})
