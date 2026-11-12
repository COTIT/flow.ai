import { describe, expect, it } from "vitest"

import { BENTO_CLASS, BENTO_GRID, bentoSize, planBands } from "./bento"

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
  it("rows are fixed tracks from 640 px: tiles never stretch to their tallest neighbour", () => {
    expect(BENTO_GRID).toMatch(/@min-\[640px\]:auto-rows-\[132px\]/)
    expect(BENTO_GRID).not.toMatch(/minmax\(132px,auto\)/)
    expect(BENTO_CLASS.wide).toMatch(/@min-\[640px\]:row-span-2/)
    expect(BENTO_CLASS.small).not.toMatch(/row-span/)
  })
})

const tile = (id: string, type: string, empty = false) => ({ id, type, empty })
const spans = (plan: ReturnType<typeof planBands>) =>
  plan.map((t) => `${t.band}:${t.id}:${t.span}${t.slim ? "s" : ""}`)

describe("planBands: rows that fill the width (bento fit)", () => {
  const pulse = [
    tile("sales-today", "metric_card"),
    tile("orders-pending", "metric_card"),
    tile("sales-trend", "line_chart"),
    tile("attention", "attention_panel"),
  ]
  it("Morning Pulse: two metrics and the chart share a row; the attention list takes the next", () => {
    expect(spans(planBands(pulse, 12))).toEqual([
      "0:sales-today:3",
      "0:orders-pending:3",
      "0:sales-trend:6",
      "1:attention:12",
    ])
  })
  it("Stock Watch: the metric and the table fill one row (no blank third)", () => {
    expect(
      spans(
        planBands(
          [
            tile("below-minimum", "metric_card"),
            tile("stock-list", "stock_list"),
          ],
          12
        )
      )
    ).toEqual(["0:below-minimum:3", "0:stock-list:9"])
  })
  it("Sales Perspective keeps its saved order; a lone last tile widens to the row", () => {
    expect(
      spans(
        planBands(
          [
            tile("sales-week", "metric_card"),
            tile("net-week", "metric_card"),
            tile("sales-daily", "bar_chart"),
            tile("vs-last-week", "comparison"),
          ],
          12
        )
      )
    ).toEqual([
      "0:sales-week:3",
      "0:net-week:3",
      "0:sales-daily:6",
      "1:vs-last-week:12",
    ])
  })
  it("three or more metrics before a big tile get their own row, widened to fill it", () => {
    expect(
      spans(
        planBands(
          [
            tile("a", "metric_card"),
            tile("b", "metric_card"),
            tile("c", "metric_card"),
            tile("t", "order_list"),
          ],
          12
        )
      )
    ).toEqual(["0:a:4", "0:b:4", "0:c:4", "1:t:12"])
    expect(
      spans(
        planBands(
          ["a", "b", "c", "d", "e"].map((id) => tile(id, "metric_card")),
          12
        )
      )
    ).toEqual(["0:a:3", "0:b:3", "0:c:3", "0:d:3", "1:e:12"])
  })
  it("two big tiles in a row share it half and half", () => {
    expect(
      spans(planBands([tile("c", "line_chart"), tile("t", "stock_list")], 12))
    ).toEqual(["0:c:6", "0:t:6"])
  })
  it("an empty component is a slim line the size of a metric, never a big box", () => {
    expect(
      spans(
        planBands(
          [
            tile("m", "metric_card"),
            tile("chart", "line_chart", true),
            tile("t", "stock_list"),
          ],
          12
        )
      )
    ).toEqual(["0:m:3", "0:chart:3s", "0:t:6"])
  })
  it("tablet (6 columns): metrics pair, big tiles take the row", () => {
    expect(spans(planBands(pulse, 6))).toEqual([
      "0:sales-today:3",
      "0:orders-pending:3",
      "1:sales-trend:6",
      "2:attention:6",
    ])
    expect(
      spans(
        planBands(
          [
            tile("below-minimum", "metric_card"),
            tile("stock-list", "stock_list"),
          ],
          6
        )
      )
    ).toEqual(["0:below-minimum:6", "1:stock-list:6"])
  })
  it("phone (2 columns): metrics pair, everything else full width", () => {
    expect(spans(planBands(pulse, 2))).toEqual([
      "0:sales-today:1",
      "0:orders-pending:1",
      "1:sales-trend:2",
      "2:attention:2",
    ])
  })
  it("every row sums to the width and the saved order is kept, whatever the mix", () => {
    const types = [
      "metric_card",
      "line_chart",
      "stock_list",
      "comparison",
      "order_list",
      "bar_chart",
    ]
    for (let seed = 1; seed < 200; seed++) {
      let x = seed
      const list = Array.from({ length: 1 + (seed % 8) }, (_, i) => {
        x = (x * 1103515245 + 12345) % 2 ** 31
        return tile(`t${i}`, types[x % types.length]!, x % 7 === 0)
      })
      for (const cols of [12, 6, 2] as const) {
        const plan = planBands(list, cols)
        expect(plan.map((t) => t.id)).toEqual(list.map((t) => t.id))
        const rows = new Map<number, number>()
        for (const t of plan) rows.set(t.band, (rows.get(t.band) ?? 0) + t.span)
        for (const [, sum] of rows) expect(sum).toBe(cols)
      }
    }
  })
})
