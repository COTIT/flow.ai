import { describe, expect, it } from "vitest"
import catalog from "./catalog.generated.json"
import en from "@/messages/en.json"
import nl from "@/messages/nl.json"
import sw from "@/messages/sw.json"
import {
  CAPABILITIES,
  allowedFor,
  followUpFor,
  moduleFor,
  pickSuggestions,
  type Available,
} from "@genesy/voice/lib/capabilities"
import {
  availableCapabilities,
  describeCapabilities,
} from "./capabilities.server"

const ops = new Map(catalog.map((op) => [op.id, op]))
const opId = /\b[a-z]+(_[a-z]+)+\b/

describe("curated capability list", () => {
  it("advertises only operations voice can run: each exists in the catalog as executable", () => {
    for (const entry of CAPABILITIES)
      for (const id of [...entry.ops, ...(entry.after ?? [])]) {
        const op = ops.get(id)
        expect(op, `${entry.id} → ${id}`).toBeDefined()
        if (entry.ops.includes(id)) {
          expect(op?.disposition, `${entry.id} → ${id}`).toBe("execute")
          expect(op, `${entry.id} → ${id}`).not.toHaveProperty(
            "availability",
            "unavailable"
          )
        }
      }
  })

  it("has a label and an example prompt in English, Dutch and Swahili, never an operation id", () => {
    for (const messages of [en, nl, sw]) {
      const texts = messages.voice.capabilities as Record<
        string,
        { label: string; prompt: string }
      >
      for (const entry of CAPABILITIES) {
        const text = texts[entry.id]
        expect(text?.label, entry.id).toBeTruthy()
        expect(text?.prompt, entry.id).toBeTruthy()
        expect(text!.label + text!.prompt).not.toMatch(opId)
      }
      for (const group of new Set(CAPABILITIES.map((c) => c.module)))
        expect(
          (messages.voice.capabilityGroups as Record<string, string>)[group]
        ).toBeTruthy()
    }
  })

  it("ids are unique", () => {
    const ids = CAPABILITIES.map((c) => c.id)
    expect(new Set(ids).size).toBe(ids.length)
  })
})

describe("server filter", () => {
  it("keeps every curated entry today and carries the catalog's rights", () => {
    const list = availableCapabilities()
    expect(list.map((c) => c.id)).toEqual(CAPABILITIES.map((c) => c.id))
    expect(list.find((c) => c.id === "email_invoice")?.rights).toEqual({
      allOf: ["invoice.send", "workspace.send-message"],
      anyOf: [],
    })
  })

  it("what-can-you-do answers at most seven plain items, grouped, limited by permission", () => {
    const owner = describeCapabilities(["workspace_owner"], "en")
    const items = owner.groups.flatMap((g) => g.items)
    expect(items.length).toBeGreaterThanOrEqual(5)
    expect(items.length).toBeLessThanOrEqual(7)
    expect(JSON.stringify(owner)).not.toMatch(opId)
    expect(owner.pages).toBe(en.voice.pagesHint)

    const clerk = describeCapabilities(["order.list"], "nl")
    expect(clerk.groups).toEqual([
      {
        group: nl.voice.capabilityGroups.orders,
        items: [
          nl.voice.capabilities.weekly_sales.label,
          nl.voice.capabilities.todays_orders.label,
        ],
      },
    ])
  })
})

const list: Available[] = availableCapabilities()

describe("choosing chips", () => {
  it("shows only what the member may do", () => {
    const may = (name: string) => name === "order.list"
    expect(allowedFor(list, may).map((c) => c.id)).toEqual([
      "weekly_sales",
      "todays_orders",
    ])
    expect(allowedFor(list, () => true)).toHaveLength(list.length)
  })

  it("puts the current page's capabilities first and spreads the rest across modules", () => {
    const picked = pickSuggestions(list, "invoices", 0)
    expect(picked).toHaveLength(4)
    expect(picked.slice(0, 2).every((c) => c.module === "invoices")).toBe(true)
    const general = pickSuggestions(list, undefined, 0)
    expect(new Set(general.map((c) => c.module)).size).toBe(4)
    expect(general.every((c) => c.priority === 1)).toBe(true)
  })

  it("varies by day, deterministically", () => {
    const today = pickSuggestions(list, undefined, 20000).map((c) => c.id)
    expect(pickSuggestions(list, undefined, 20000).map((c) => c.id)).toEqual(
      today
    )
    expect(
      pickSuggestions(list, undefined, 20001).map((c) => c.id)
    ).not.toEqual(today)
  })

  it("offers one follow-up from a fixed mapping, only when allowed", () => {
    expect(followUpFor(list, "invoices_get")?.id).toBe("email_invoice")
    expect(followUpFor(list, "products_get")?.id).toBe("change_price")
    expect(followUpFor(list, "orders_list")).toBeUndefined()
    expect(followUpFor([], "invoices_get")).toBeUndefined()
  })

  it("reads the module from the page the assistant was opened on", () => {
    expect(moduleFor("/invoices")).toBe("invoices")
    expect(moduleFor("/invoices/0195")).toBe("invoices")
    expect(moduleFor("/till/sales")).toBe("till")
    expect(moduleFor("/settings/invites")).toBe("settings")
    expect(moduleFor("/lots")).toBeUndefined()
    expect(moduleFor("https://evil.test/invoices")).toBeUndefined()
    expect(moduleFor(undefined)).toBeUndefined()
  })
})
