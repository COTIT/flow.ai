import { describe, expect, it } from "vitest"
import type { TranscriptRow } from "./helpers"
import type { ResultView } from "./views"
import {
  answerResultIds,
  latestChart,
  fitReportChart,
  pdfReportTables,
  reportTables,
  retainedSelection,
  reportItems,
  reportSummary,
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
  it("presents only supplied sales totals, keeping currencies and periods separate", () => {
    const first = row("totals", {
      ...sales,
      rows: [
        {
          ...sales.rows[0]!,
          gross: -125,
          net: null,
          vat: null,
          refunded: 0,
          orders: 0,
        },
        { ...sales.rows[0]!, currency: "JPY", decimals: 0, gross: 300 },
      ],
    })
    first.views!.unshift({
      kind: "records",
      resource: "orders",
      total: 1,
      href: "/orders",
      rows: [],
    })
    const rows = [
      first,
      row("later", { ...sales, from: "2026-12-01", to: "2026-12-02" }),
      row("daily", {
        kind: "sales_daily",
        from: sales.from,
        to: sales.to,
        time_zone: "UTC",
        currencies: [
          {
            currency: "USD",
            decimals: 2,
            days: [{ date: sales.from, gross: 9999, net: 9000, orders: 1 }],
          },
        ],
      }),
    ]
    const items = reportItems(
      rows,
      rows.map((r) => r.id),
      (view) => (view.kind === "sales" ? "Sales totals" : "Other result")
    )
    const summary = reportSummary(items, "en-US")
    expect(summary.map((s) => [s.title, s.currency, s.period])).toEqual([
      ["Sales totals", "USD", "2026-11-01 – 2026-11-02"],
      ["Sales totals", "JPY", "2026-11-01 – 2026-11-02"],
      ["Sales totals", "USD", "2026-12-01 – 2026-12-02"],
    ])
    expect(summary[0]!.metrics).toEqual([
      { key: "gross", raw: -125, value: "-$1.25" },
      { key: "orders", raw: 0, value: "0" },
      { key: "refunded", raw: 0, value: "$0.00" },
    ])
    expect(summary[0]).toMatchObject({
      rowId: "totals",
      viewIndex: 1,
      decimals: 2,
    })
    expect(items[0]!.viewTitles).toEqual(["Other result", "Sales totals"])
    expect(
      items[0]!.tables.find((t) => t.key === "sales")?.rows[0]?.gross
    ).toBe(-125)
  })

  it("fits a tall chart and legend on a PDF page without changing its aspect ratio", () => {
    const fitted = fitReportChart(720, 1200, 269, 170)
    expect(fitted.height).toBe(170)
    expect(fitted.width).toBe(102)
    expect(fitReportChart(720, 320, 269, 170).width).toBe(269)
  })

  it("formats PDF money from exact units, without changing raw workbook tables", () => {
    const raw = reportTables({
      ...sales,
      rows: [
        sales.rows[0]!,
        {
          currency: "JPY",
          decimals: 0,
          gross: -125,
          net: 0,
          vat: 0,
          refunded: null,
          orders: 0,
        },
      ],
    })
    const pdf = pdfReportTables(raw, "en-US")
    expect(pdf).toHaveLength(1)
    expect(pdf[0]?.columns).not.toContain("decimals")
    expect(pdf[0]?.rows[0]).toMatchObject({
      gross: "$1.00",
      net: "$0.85",
      vat: "$0.15",
      refunded: null,
    })
    expect(pdf[0]?.rows[1]).toMatchObject({ gross: "-¥125", net: "¥0" })
    expect(raw[1]?.rows[0]?.gross).toBe(100)
    const nested = pdfReportTables(
      [
        {
          key: "lines",
          columns: [
            "quantity",
            "money.minor",
            "money.currency",
            "money.decimals",
          ],
          rows: [
            {
              quantity: "2.0000",
              "money.minor": 125,
              "money.currency": "EUR",
              "money.decimals": 2,
            },
          ],
        },
      ],
      "en-US"
    )
    expect(nested[0]?.rows[0]).toMatchObject({
      quantity: "2.0000",
      "money.minor": "€1.25",
    })
    expect(nested[0]?.columns).not.toContain("money.decimals")
  })

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
