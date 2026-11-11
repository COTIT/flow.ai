import { describe, expect, it } from "vitest"

import { BENTO_CLASS, bentoSize } from "./bento"

describe("bento sizing (one rule, every scene)", () => {
  it("metric cards and comparisons are small", () => {
    expect(bentoSize("metric_card", 4)).toBe("small")
    expect(bentoSize("comparison", 4)).toBe("small")
  })
  it("charts are wide and two rows tall", () => {
    expect(bentoSize("line_chart", 4)).toBe("chart")
    expect(bentoSize("bar_chart", 4)).toBe("chart")
  })
  it("lists and the attention panel are wide", () => {
    for (const t of [
      "order_list",
      "stock_list",
      "invoice_table",
      "report_list",
      "attention_panel",
    ])
      expect(bentoSize(t, 3)).toBe("wide")
  })
  it("a scene of one component takes the full row", () => {
    expect(bentoSize("attention_panel", 1)).toBe("full")
    expect(bentoSize("line_chart", 1)).toBe("full")
  })
  it("an unknown type is wide (never squeezed)", () => {
    expect(bentoSize("something_new", 3)).toBe("wide")
  })
  it("spans 3/3/1 small, 6/6/2 wide, the full row for full", () => {
    expect(BENTO_CLASS.small).toMatch(
      /col-span-1\b.*@min-\[640px\]:col-span-3\b.*@min-\[1024px\]:col-span-3\b/
    )
    expect(BENTO_CLASS.wide).toMatch(
      /col-span-2\b.*@min-\[640px\]:col-span-6\b.*@min-\[1024px\]:col-span-6\b/
    )
    expect(BENTO_CLASS.chart).toMatch(/row-span-2/)
    expect(BENTO_CLASS.full).toMatch(/col-span-full/)
  })
})
