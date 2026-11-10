import {
  blockedHandler,
  expectInjectedScriptBlocked,
  expectSecurityHeaders,
} from "../../src/security"
import { test } from "../../src/test"

test("pages carry a fresh-nonce CSP (same as Office), nosniff and a referrer policy", async ({
  page,
}) => {
  await expectSecurityHeaders(page, "/login", [
    "https://api.elevenlabs.io",
    "wss://api.elevenlabs.io",
  ])
})

test.describe("injected markup", () => {
  test.use({ allow: blockedHandler })

  test("an inline handler is refused by the CSP and never runs", async ({
    page,
  }) => {
    await page.goto("/login")
    await expectInjectedScriptBlocked(page)
  })
})
