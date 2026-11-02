import { describe, expect, it } from "vitest"

import { appLinks, contactHref } from "./site"

describe("contactHref", () => {
  it("falls back to the placeholder address until one is configured", () => {
    expect(contactHref(undefined)).toBe(
      "mailto:hello@genesy.app?subject=Genesy%20for%20my%20organisation"
    )
  })

  it("uses the configured address, trimmed", () => {
    expect(contactHref("  team@example.org ")).toMatch(/^mailto:team@example\.org\?/)
  })
})

describe("appLinks", () => {
  it("points at the production hosts by default", () => {
    expect(appLinks({})).toEqual({
      shop: "https://shop.genesy.app",
      office: "https://office.genesy.app",
      field: "https://field.genesy.app",
      till: "https://till.genesy.app",
    })
  })

  it("takes a host per app from the environment", () => {
    expect(
      appLinks({ NEXT_PUBLIC_SHOP_URL: "https://staging-shop.genesy.app" }).shop
    ).toBe("https://staging-shop.genesy.app")
  })
})
