import { expect, test } from "@playwright/test"

import { safeNext } from "../../../../apps/office/lib/safe-next"

// The office has no unit runner, so the pure helper's cases run here (no page needed).
test("safeNext keeps same-site paths and refuses everything else", () => {
  for (const bad of [
    null,
    "",
    "orders",
    "//evil.com",
    "/\\evil.com",
    "https://evil.com",
    "/%2F%2Fevil.com",
    "/%2f%2fevil.com",
    "/%5Cevil.com",
    "/%5cevil.com",
    // Browsers drop tabs and newlines from URLs, which would turn these into "//evil.com".
    "/\t/evil.com",
    "/\n/evil.com",
    "/%09/evil.com",
    "javascript:alert(1)",
    "/%E0%A4%A",
  ])
    expect(safeNext(bad), String(bad)).toBe("/")
  for (const good of ["/", "/orders", "/accept-invitation?code=abc"])
    expect(safeNext(good)).toBe(good)
})
