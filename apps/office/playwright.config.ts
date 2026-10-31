import { defineConfig, devices } from "@playwright/test"

// shortcut: office-only config; move the shared parts to @genesy/e2e when it lands on main.
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "github" : "list",
  use: { baseURL: "http://localhost:3001", trace: "retain-on-failure" },
  projects: [
    { name: "desktop", use: { ...devices["Desktop Chrome"] } },
    {
      name: "mobile",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 375, height: 812 },
      },
    },
  ],
  webServer: {
    // The fake API answers in the browser; nothing listens on API_URL.
    command: "pnpm build && pnpm start",
    url: "http://localhost:3001/login",
    reuseExistingServer: !process.env.CI,
    timeout: 240_000,
  },
})
