import { rmSync } from "node:fs"
import { defineConfig } from "@playwright/test"

// Voice probe (README.md here): slow, paid and model-dependent, so never part of CI.
// Only the main process clears the previous results (unless resuming); workers load this file too.
if (!process.env.TEST_WORKER_INDEX && process.env.VOICE_PROBE_RESUME !== "1")
  rmSync(new URL(`./out/${process.env.VOICE_PROBE_BANK ?? "scenarios"}/results`, import.meta.url), { recursive: true, force: true })

export default defineConfig({
  testDir: ".",
  testMatch: "probe.spec.ts",
  // One conversation at a time: the provider agent is shared and every turn is billed.
  workers: 1,
  retries: 0,
  timeout: 6 * 60_000,
  reporter: [["list"]],
  use: {
    viewport: { width: 1280, height: 900 },
    // Deliberately not the account's zone: a Manila time on screen means browser time leaked.
    timezoneId: "Asia/Manila",
    trace: "retain-on-failure",
  },
})
