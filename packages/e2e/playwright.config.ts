import { defineConfig } from "@playwright/test"

import { APP_KEY, MOCK_ORIGIN } from "./mock/fixtures.mjs"
import { projectsFor } from "./src/projects"

const ci = Boolean(process.env.CI)
const shop = "http://127.0.0.1:3200"
const office = "http://127.0.0.1:3201"

/**
 * Headless end-to-end runs against production builds (`next start`), so `pnpm turbo run e2e`
 * builds first. Ports 3200/3201/8199 are kept apart from the dev servers.
 */
export default defineConfig({
  forbidOnly: ci,
  retries: ci ? 1 : 0,
  workers: ci ? 2 : undefined,
  reporter: ci ? [["list"], ["html", { open: "never" }]] : [["list"]],
  use: { trace: "on-first-retry" },
  projects: [
    ...projectsFor("shop", { testDir: "tests/shop", baseURL: shop }),
    ...projectsFor("office", { testDir: "tests/office", baseURL: office }),
  ],
  webServer: [
    {
      command: "node mock/server.mjs",
      url: `${MOCK_ORIGIN}/health`,
      reuseExistingServer: false,
    },
    {
      command:
        "pnpm --filter @genesy/shop exec next start --port 3200 --hostname 127.0.0.1",
      url: shop,
      reuseExistingServer: false,
      env: {
        API_URL: `${MOCK_ORIGIN}/v1`,
        STOREFRONT_APP_KEY: APP_KEY,
        FILES_ORIGIN: MOCK_ORIGIN,
        SITE_URL: shop,
      },
    },
    {
      // The office calls the API from the browser; specs answer with mockApi. Port 9 is
      // closed, so a call that slips past the mock fails instead of reaching a backend.
      command:
        "pnpm --filter @genesy/office exec next start --port 3201 --hostname 127.0.0.1",
      url: `${office}/login`,
      reuseExistingServer: false,
      env: { API_URL: "http://127.0.0.1:9/v1" },
    },
  ],
})
