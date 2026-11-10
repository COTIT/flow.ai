import { defineConfig } from "@playwright/test"

import config from "./playwright.config"

/** Existing Shop regressions without starting unrelated apps. */
export default defineConfig({
  ...config,
  projects: config.projects?.filter((project) =>
    project.name?.startsWith("shop-")
  ),
  webServer: Array.isArray(config.webServer)
    ? config.webServer.slice(0, 2)
    : config.webServer,
})
