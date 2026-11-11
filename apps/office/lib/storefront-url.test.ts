import { describe, expect, it } from "vitest"

import { storefrontUrl } from "./storefront-url"

const id = "01950000-0000-7000-8000-000000000001"
const connection = { id, active: true }

describe("workspace storefront URLs", () => {
  it("uses the active connection for the shop and passport", () => {
    expect(storefrontUrl("https://shop.example/", connection)).toBe(
      `https://shop.example/s/${id}`
    )
    expect(
      storefrontUrl("https://shop.example", connection, "7K3M9Q2XRT")
    ).toBe(`https://shop.example/s/${id}/b/7K3M9Q2XRT`)
    expect(storefrontUrl("http://localhost:3200", connection, "a/b")).toBe(
      `http://localhost:3200/s/${id}/b/a%2Fb`
    )
  })

  it("never falls back to a global shop or the Office origin", () => {
    for (const value of [
      undefined,
      null,
      { id: null, active: false },
      { id, active: false },
      { id: "../another-workspace", active: true },
    ])
      expect(
        storefrontUrl("https://shop.example", value, "7K3M9Q2XRT")
      ).toBeNull()
    for (const base of [
      undefined,
      "",
      "invalid",
      "javascript:alert(1)",
      "https://user:secret@shop.example",
      "https://shop.example?other=1",
      "https://shop.example/#other",
    ])
      expect(storefrontUrl(base, connection)).toBeNull()
  })
})
