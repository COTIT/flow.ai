import { existsSync } from "node:fs"
import { join } from "node:path"
import { describe, expect, it } from "vitest"
import en from "@/messages/en.json"
import nl from "@/messages/nl.json"
import sw from "@/messages/sw.json"
import { rightFor } from "../routes"
import { PAGES, openPage, pageTool, PAGE_TOOL } from "./pages"

const appDir = join(__dirname, "../../app/(app)")
const uuid = "0195aaaa-0000-7000-8000-000000000001"

describe("office pages index", () => {
  it("points only at real office pages", () => {
    for (const page of PAGES) {
      const dir = join(appDir, page.route, page.record ? "[id]" : "")
      expect(existsSync(join(dir, "page.tsx")), page.id).toBe(true)
    }
  })

  it("has a name and purpose in English, Dutch and Swahili", () => {
    for (const messages of [en, nl, sw])
      for (const page of PAGES) {
        const text = (
          messages.voice.pages as Record<
            string,
            { name: string; purpose: string }
          >
        )[page.id]
        expect(text?.name, page.id).toBeTruthy()
        expect(text?.purpose, page.id).toBeTruthy()
      }
  })

  it("tells the model every page id and purpose, and never a path", () => {
    const page = pageTool.body.properties.page
    expect(page.enum).toEqual(PAGES.map((p) => p.id))
    expect(page.description).toContain(en.voice.pages.product.purpose)
    expect(JSON.stringify(pageTool)).not.toMatch(/"\/[a-z]/)
    expect(pageTool).toMatchObject({ id: PAGE_TOOL, write: false })
  })
})

describe("opening a page", () => {
  it("returns the page as a link row named in the session language", () => {
    expect(openPage({ page: "products" }, ["product.list"], "nl")).toEqual({
      status: "completed",
      data: { page: nl.voice.pages.products.name },
      links: [{ label: nl.voice.pages.products.name, href: "/products" }],
      narration: nl.voice.pages.products.name,
    })
    expect(
      openPage(
        { page: "invoice", recordId: uuid.toUpperCase() },
        ["workspace_admin"],
        "en"
      ).links
    ).toEqual([
      { label: en.voice.pages.invoice.name, href: `/invoices/${uuid}` },
    ])
    expect(openPage({ page: "overview" }, [], "sw").links?.[0]?.href).toBe("/")
  })

  it("uses the same right as the office route", () => {
    for (const page of PAGES) {
      const right = rightFor(page.route)
      const args = page.record
        ? { page: page.id, recordId: uuid }
        : { page: page.id }
      if (right)
        expect(() => openPage(args, [], "en"), page.id).toThrow(/permission/)
      expect(() => openPage(args, ["workspace_owner"], "en")).not.toThrow()
    }
  })

  it("refuses unknown pages, built paths, and record ids that are not uuids", () => {
    const all = ["workspace_owner"]
    expect(() => openPage({ page: "/settings/app-keys" }, all, "en")).toThrow()
    expect(() => openPage({ page: "elsewhere" }, all, "en")).toThrow()
    expect(() => openPage({ page: "invoice" }, all, "en")).toThrow()
    expect(() =>
      openPage({ page: "invoice", recordId: "../settings" }, all, "en")
    ).toThrow()
    expect(() =>
      openPage({ page: "products", recordId: uuid }, all, "en")
    ).toThrow()
  })
})
