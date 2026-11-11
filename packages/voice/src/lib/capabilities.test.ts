import { describe, expect, it } from "vitest"

import { sceneKey, statusOf, type FlowCapability } from "./capabilities"

const cap = (over: Partial<FlowCapability>): FlowCapability => ({
  id: "x",
  group: "operate",
  ops: ["stock_list"],
  rights: ["stock.list"],
  write: false,
  entry: { href: "/scenes/stock" },
  scenes: [],
  ...over,
})
const may = (granted: string[]) => (right: string) => granted.includes(right)

describe("statusOf", () => {
  it("planned stays planned, whatever the rights", () => {
    expect(statusOf(cap({ planned: true, ops: [], rights: [] }), may([]))).toBe(
      "planned"
    )
  })

  it("restricted when any right is missing", () => {
    expect(
      statusOf(
        cap({ rights: ["stock.list", "order.list"] }),
        may(["stock.list"])
      )
    ).toBe("restricted")
  })

  it("a write asked of Genesy needs approval", () => {
    expect(
      statusOf(
        cap({
          write: true,
          entry: { prompt: true },
          rights: ["stock.set_stock_levels"],
        }),
        may(["stock.set_stock_levels"])
      )
    ).toBe("approval")
  })

  it("reads, and the person's own direct settings, are available", () => {
    expect(statusOf(cap({}), may(["stock.list"]))).toBe("available")
    expect(statusOf(cap({ write: true, rights: [] }), may([]))).toBe(
      "available"
    )
  })
})

describe("sceneKey", () => {
  it("maps Flow routes to scene keys", () => {
    expect(sceneKey("/")).toBe("home")
    expect(sceneKey("/scenes/stock")).toBe("stock")
    expect(sceneKey("/scenes/pulse/")).toBe("pulse")
    expect(sceneKey("/explore")).toBe("explore")
    expect(sceneKey("/storefront")).toBe("studio")
    expect(sceneKey("/elsewhere")).toBeUndefined()
  })
})
