import { describe, expect, it } from "vitest"
import type { TranscriptRow } from "./helpers"
import type { ResultView } from "./views"
import {
  answerResultIds,
  latestChart,
  reportTables,
  retainedSelection,
} from "./report"

const row = (id: string, view: ResultView, requestId = "request-1") =>
  ({
    id,
    role: "result",
    text: "",
    views: [view],
    requestId,
  }) as TranscriptRow & { requestId: string }
const sales: ResultView = {
  kind: "sales",
  from: "2026-11-01",
  to: "2026-11-02",
  source: null,
  rows: [
    {
      currency: "USD",
      decimals: 2,
      gross: 100,
      net: 85,
      vat: 15,
      refunded: null,
      orders: 2,
    },
  ],
}

describe("report snapshots", () => {
  it("preserves all sales fields and keeps currencies separate", () => {
    const tables = reportTables({
      ...sales,
      rows: [
        ...sales.rows,
        { ...sales.rows[0]!, currency: "TZS", decimals: 0 },
      ],
    })
    expect(tables[1]?.rows).toEqual([
      {
        currency: "USD",
        decimals: 2,
        gross: 100,
        net: 85,
        vat: 15,
        refunded: null,
        orders: 2,
      },
      {
        currency: "TZS",
        decimals: 0,
        gross: 100,
        net: 85,
        vat: 15,
        refunded: null,
        orders: 2,
      },
    ])
  })

  it("never converts exact decimal quantities to drawing numbers", () => {
    const tables = reportTables({
      kind: "stock",
      scope: "list",
      total: 50,
      href: "/stock",
      groups: [
        {
          title: "Main",
          rows: [
            {
              label: "=SUM(A1:A2)",
              detail: null,
              level: "9007199254740993.0100",
              available: "-0.0001",
              reserved: "0.0000",
              minimum: 0,
              href: null,
            },
          ],
        },
      ],
    })
    expect(tables[1]?.rows[0]).toMatchObject({
      warehouse: "Main",
      label: "=SUM(A1:A2)",
      level: "9007199254740993.0100",
      available: "-0.0001",
      reserved: "0.0000",
    })
    expect(tables[0]?.rows[0]).toMatchObject({ total: 50 })
  })

  it("exports document quantity and money together", () => {
    const tables = reportTables({
      kind: "document",
      resource: "orders",
      title: "ORD-1",
      status: "open",
      href: "/orders/1",
      facts: [],
      lineMoney: "incl",
      lines: [
        {
          label: "Honey",
          quantity: "2.00",
          money: { minor: 125, currency: "EUR", decimals: 2 },
        },
      ],
      totals: [],
    })
    expect(tables.find((t) => t.key === "lines")?.rows[0]).toEqual({
      label: "Honey",
      quantity: "2.00",
      "money.minor": 125,
      "money.currency": "EUR",
      "money.decimals": 2,
    })
  })

  it("counts results by request identity, not adjacent transcript rows", () => {
    const rows = [
      row("r1", sales),
      { id: "agent", role: "agent", text: "summary" } as TranscriptRow,
      row("r2", sales),
      row("old", sales, "request-0"),
    ]
    expect(answerResultIds(rows, "request-1")).toEqual(["r1", "r2"])
    expect(answerResultIds(rows, null)).toEqual([])
    expect(retainedSelection(["r1", "evicted"], rows)).toEqual(["r1"])
  })

  it("latest chart skips non-chart result widgets", () => {
    const rows = [
      row("r1", sales),
      row("r2", {
        kind: "records",
        resource: "orders",
        total: 0,
        href: "/orders",
        rows: [],
      }),
    ]
    expect(latestChart(rows)?.id).toBe("r1")
    expect(latestChart([])).toBeUndefined()
  })
})
