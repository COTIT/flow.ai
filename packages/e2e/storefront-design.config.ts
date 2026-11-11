import { defineConfig } from "@playwright/test"

import {
  APP_KEY,
  FILES_ORIGIN,
  MOCK_ORIGIN,
  PORT_OFFSET,
} from "./mock/fixtures.mjs"
import { projectsFor, tablets } from "./src/projects"

const port = 3300 + PORT_OFFSET
const shop = `http://127.0.0.1:${port}`

/** Isolated published-home scenarios; legacy Shop tests keep renderer disabled. */
export default defineConfig({
  forbidOnly: Boolean(process.env.CI),
  workers: 1,
  retries: 0,
  use: { trace: "retain-on-failure" },
  projects: [
    ...projectsFor("shop-design", {
      testDir: "tests/shop-design",
      baseURL: shop,
    }),
    {
      name: "shop-design-tablet",
      testDir: "tests/shop-design",
      use: { ...tablets["ipad-portrait"], baseURL: shop },
    },
  ],
  webServer: [
    {
      command: "node mock/server.mjs",
      url: `${MOCK_ORIGIN}/health`,
      reuseExistingServer: false,
      env: { E2E_DESIGN: "true" },
    },
    {
      command: `pnpm --filter @genesy/shop exec next start --port ${port} --hostname 127.0.0.1`,
      url: shop,
      reuseExistingServer: false,
      env: {
        API_URL: `${MOCK_ORIGIN}/v1`,
        STOREFRONT_APP_KEY: APP_KEY,
        FILES_ORIGIN,
        SITE_URL: shop,
        SHOP_DESIGN_ENABLED: "true",
        FLOW_URL: "https://flow.example.test",
      },
    },
  ],
})
