import { describe, expect, it } from "vitest"

import { FLOW_CAPABILITIES, type FlowCapability } from "./capabilities"
import { nextActions, rankPills } from "./rank"

const cap = (
  id: string,
  over: Partial<FlowCapability> = {}
): FlowCapability => ({
  id,
  group: "operate",
  ops: ["stock_list"],
  rights: [],
  write: false,
  entry: { prompt: true },
  scenes: [],
  ...over,
})

const list = [
  cap("a", { scenes: ["home"] }),
  cap("b", { scenes: ["home", "stock"] }),
  cap("c", { attention: "stock.below_minimum" }),
  cap("d", { attention: "orders.open" }),
  cap("e"),
  cap("f", { rights: ["order.list"], scenes: ["home"] }),
  cap("g", { planned: true, ops: [], scenes: ["home"] }),
  cap("stock", { entry: { href: "/scenes/stock" }, scenes: ["home"] }),
]
const all = () => true
const base = { list, scene: "home" as const, may: all, uses: {}, attention: [] }

describe("rankPills", () => {
  it("fills the slots in order: frequent, contextual, attention, unfamiliar", () => {
    const ids = rankPills({
      ...base,
      uses: { e: { n: 3, last: 5 }, a: { n: 1, last: 9 } },
      attention: ["orders.open"],
    })
    // e frequent · a contextual · d attention · b first never used
    expect(ids).toEqual(["e", "a", "d", "b"])
  })

  it("needs three uses before something counts as frequent", () => {
    const ids = rankPills({ ...base, uses: { e: { n: 2, last: 5 } } })
    expect(ids[0]).toBe("a")
    expect(ids).not.toContain("e")
  })

  it("breaks frequency ties by the latest use", () => {
    const ids = rankPills({
      ...base,
      uses: { e: { n: 4, last: 1 }, d: { n: 4, last: 2 } },
    })
    expect(ids[0]).toBe("d")
  })

  it("never offers what the role cannot use, nor planned entries", () => {
    const may = (right: string) => right !== "order.list"
    const ids = rankPills({ ...base, may, max: 8 })
    expect(ids).not.toContain("f")
    expect(ids).not.toContain("g")
    expect(rankPills({ ...base, max: 8 })).toContain("f")
  })

  it("does not offer the screen already open", () => {
    expect(rankPills({ ...base, max: 8 })).toContain("stock")
    expect(rankPills({ ...base, scene: "stock", max: 8 })).not.toContain(
      "stock"
    )
  })

  it("attention follows the Flow noticed order", () => {
    const first = rankPills({
      ...base,
      attention: ["orders.open", "stock.below_minimum"],
    })
    const second = rankPills({
      ...base,
      attention: ["stock.below_minimum", "orders.open"],
    })
    // no frequent pill yet: contextual, attention, unfamiliar
    expect(first.slice(0, 3)).toEqual(["a", "d", "b"])
    expect(second.slice(0, 3)).toEqual(["a", "c", "b"])
  })

  it("fills empty slots with more contextual entries, then the registry order", () => {
    const used = Object.fromEntries(list.map((c) => [c.id, { n: 1, last: 1 }]))
    expect(rankPills({ ...base, uses: used })).toEqual(["a", "b", "f", "stock"])
    expect(rankPills({ ...base, scene: "orders", uses: used })).toEqual([
      "a",
      "b",
      "c",
      "d",
    ])
  })

  it("is deterministic and capped", () => {
    const input = { ...base, attention: ["stock.below_minimum"], max: 3 }
    expect(rankPills(input)).toEqual(rankPills(input))
    expect(rankPills(input)).toHaveLength(3)
  })
})

describe("nextActions", () => {
  it("offers relevant read-only sales follow-ups only with sales permission", () => {
    const next = nextActions(FLOW_CAPABILITIES, "products_top_sellers", all)
    expect(next.map((c) => c.id)).toContain("compare_sales_periods")
    expect(next.map((c) => c.id)).toContain("sales_trend_followup")
    expect(next.every((c) => !c.write)).toBe(true)
    expect(nextActions(FLOW_CAPABILITIES, "products_top_sellers", () => false)).toEqual([])
  })
  const after = [
    cap("x", { after: ["stock_set_levels"] }),
    cap("y", { after: ["stock_set_levels"], rights: ["order.list"] }),
    cap("z", { after: ["products_update"] }),
  ]
  it("offers what follows the completed operation, only when allowed", () => {
    expect(
      nextActions(after, "stock_set_levels", all).map((c) => c.id)
    ).toEqual(["x", "y"])
    expect(
      nextActions(after, "stock_set_levels", (r) => r !== "order.list").map(
        (c) => c.id
      )
    ).toEqual(["x"])
    expect(nextActions(after, undefined, all)).toEqual([])
  })
})
