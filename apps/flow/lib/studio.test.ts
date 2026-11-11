import { afterEach, describe, expect, it, vi } from "vitest"

import {
  briefText,
  forgetTokens,
  linkView,
  rememberToken,
  revisionLabel,
  tokenFor,
} from "./studio"

const link = (
  status: string,
  over: Partial<Record<string, string | null>> = {}
) => ({
  link_id: `l-${status}`,
  revision_id: "r2",
  status,
  created_at: "2026-11-11T10:00:00Z",
  created_by: "u1",
  expires_at: "2026-11-13T10:00:00Z",
  revoked_at: null,
  ...over,
})

describe("revisionLabel", () => {
  const page = {
    data: [{ revision_id: "r3" }, { revision_id: "r2" }, { revision_id: "r1" }],
    meta: { total: 3, from: 1 },
  }
  it("numbers revisions newest first, append-only", () => {
    expect(revisionLabel(page, "r3")).toBe("v3")
    expect(revisionLabel(page, "r1")).toBe("v1")
  })
  it("answers a dash for an unknown or missing revision", () => {
    expect(revisionLabel(page, "r9")).toBe("—")
    expect(revisionLabel(page, null)).toBe("—")
    expect(revisionLabel(undefined, "r1")).toBe("—")
  })
  it("counts from the page's first row", () => {
    expect(
      revisionLabel(
        { data: [{ revision_id: "r1" }], meta: { total: 51, from: 51 } },
        "r1"
      )
    ).toBe("v1")
  })
})

describe("linkView", () => {
  it("no link: none", () => {
    expect(linkView([], null)).toEqual({ state: "none", link: null })
  })
  it("active with the token in memory: open", () => {
    const l = link("active")
    expect(linkView([l], "t")).toEqual({ state: "active", link: l })
  })
  it("active without the token (reload, other tab): shown once", () => {
    const l = link("active")
    expect(linkView([l], null)).toEqual({ state: "hidden", link: l })
  })
  it.each(["expired", "revoked", "ended"])("%s", (status) => {
    const l = link(status)
    expect(linkView([l], "t")).toEqual({ state: status, link: l })
  })
  it("the newest link decides (API lists newest first)", () => {
    const newest = link("active", { link_id: "new" })
    expect(linkView([newest, link("revoked")], "t").link).toBe(newest)
  })
})

describe("briefText", () => {
  it("names the chosen areas, structure and direction", () => {
    expect(
      briefText({
        areas: ["Hero", "Typography"],
        keepStructure: true,
        direction: "editorial",
      })
    ).toBe(
      "Redesign my storefront: change the hero and typography; keep the existing product structure; use an editorial layout."
    )
  })
  it("one area, change structure, product-focused", () => {
    expect(
      briefText({
        areas: ["Colours"],
        keepStructure: false,
        direction: "product",
      })
    ).toBe(
      "Redesign my storefront: change the colours; the product structure may change; use a product-focused layout."
    )
  })
  it("three areas read as a list", () => {
    expect(
      briefText({
        areas: ["Hero", "Product layout", "Navigation"],
        keepStructure: true,
        direction: "product",
      })
    ).toContain("change the hero, product layout and navigation;")
  })
})

describe("token memory", () => {
  afterEach(() => {
    forgetTokens()
    vi.unstubAllGlobals()
  })
  it("keeps one token per revision in memory only, never in storage or cookies", () => {
    const storage = () => ({ setItem: vi.fn(), getItem: vi.fn() })
    const local = storage()
    const session = storage()
    const doc = { cookie: "" }
    vi.stubGlobal("localStorage", local)
    vi.stubGlobal("sessionStorage", session)
    vi.stubGlobal("document", doc)
    rememberToken("r2", "a".repeat(64))
    expect(tokenFor("r2")).toBe("a".repeat(64))
    expect(tokenFor("r1")).toBeNull()
    expect(local.setItem).not.toHaveBeenCalled()
    expect(session.setItem).not.toHaveBeenCalled()
    expect(doc.cookie).toBe("")
  })
  it("a new draft drops older revisions' tokens", () => {
    rememberToken("r1", "a".repeat(64))
    rememberToken("r2", "b".repeat(64))
    expect(tokenFor("r1")).toBeNull()
    expect(tokenFor("r2")).toBe("b".repeat(64))
  })
})
