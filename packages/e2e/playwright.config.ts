import { defineConfig } from "@playwright/test"

import { APP_KEY, MOCK_ORIGIN, PORT_OFFSET } from "./mock/fixtures.mjs"
import { projectsFor } from "./src/projects"

const ci = Boolean(process.env.CI)
const shopPort = 3200 + PORT_OFFSET
const officePort = 3201 + PORT_OFFSET
const fieldPort = 3202 + PORT_OFFSET
const tillPort = 3203 + PORT_OFFSET
const shop = `http://127.0.0.1:${shopPort}`
const office = `http://127.0.0.1:${officePort}`
const field = `http://127.0.0.1:${fieldPort}`
const till = `http://127.0.0.1:${tillPort}`

/**
 * Headless end-to-end runs against production builds (`next start`), so `pnpm turbo run e2e`
 * builds first. Ports 3200-3203/8199 (+ E2E_PORT_OFFSET) are kept apart from the dev servers.
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
    // The PWA's service worker would route page requests past page.route, so mockApi needs it off.
    ...projectsFor("field", { testDir: "tests/field", baseURL: field }).map((p) => ({
      ...p,
      use: { ...p.use, serviceWorkers: "block" as const },
    })),
    ...projectsFor("till", { testDir: "tests/till", baseURL: till }),
  ],
  webServer: [
    {
      command: "node mock/server.mjs",
      url: `${MOCK_ORIGIN}/health`,
      reuseExistingServer: false,
    },
    {
      command: `pnpm --filter @genesy/shop exec next start --port ${shopPort} --hostname 127.0.0.1`,
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
      command: `pnpm --filter @genesy/office exec next start --port ${officePort} --hostname 127.0.0.1`,
      url: `${office}/login`,
      reuseExistingServer: false,
      env: { API_URL: "http://127.0.0.1:9/v1" },
    },
    {
      // Field (Vite) calls the API from the browser too; mockApi answers, port 9 catches leaks.
      command:
        `pnpm --filter @genesy/field exec vite preview --port ${fieldPort} --host 127.0.0.1 --strictPort`,
      url: `${field}/login`,
      reuseExistingServer: false,
      env: { API_URL: "http://127.0.0.1:9/v1" },
    },
    {
      // The till calls the API from the browser like the office; mockApi answers, port 9 catches leaks.
      command: `pnpm --filter @genesy/till exec next start --port ${tillPort} --hostname 127.0.0.1`,
      url: `${till}/setup`,
      reuseExistingServer: false,
      env: { API_URL: "http://127.0.0.1:9/v1" },
    },
  ],
})
