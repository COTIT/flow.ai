import { describe, expect, it } from "vitest"
import { reportItems, type ReportLabels } from "./report"
import { reportWorkbook } from "./report-excel"
import JSZip from "jszip"

const labels: ReportLabels = {
  title: "Report",
  snapshot: "Displayed snapshot; may be partial.",
  captured: "Captured",
  widget: "Widget",
  period: "Period",
  columns: (key) => key,
}

describe("reportWorkbook", () => {
  it("keeps separate same-currency sales rows with different decimal precision", async () => {
    const items = reportItems(
      [
        {
          id: "precision",
          role: "result",
          text: "",
          views: [
            {
              kind: "sales",
              presentation: "kpi",
              from: "2026-10-01",
              to: "2026-11-01",
              source: "all",
              rows: [
                {
                  currency: "USD",
                  decimals: 2,
                  gross: 12345,
                  net: null,
                  vat: null,
                  refunded: null,
                  orders: 2,
                },
                {
                  currency: "USD",
                  decimals: 0,
                  gross: 200,
                  net: null,
                  vat: null,
                  refunded: null,
                  orders: 3,
                },
              ],
            },
          ],
        },
      ],
      ["precision"],
      () => "Sales"
    )
    const { Workbook } = await import("exceljs")
    const read = new Workbook()
    await read.xlsx.load((await reportWorkbook(items, labels)) as never)
    const summary = read.worksheets[0]!
    expect(summary.getCell("B11").value).toEqual({
      formula: "'1 Sales'!C13",
      result: 123.45,
    })
    expect(summary.getCell("B17").value).toEqual({
      formula: "'1 Sales'!C14",
      result: 200,
    })
    expect(summary.getCell("B11").numFmt).toContain("0.00")
    expect(summary.getCell("B17").numFmt).not.toContain("0.00")
  })

  it("keeps chart XML valid for control characters while retaining source labels", async () => {
    const items = reportItems(
      [
        {
          id: "control",
          role: "result",
          text: "",
          views: [
            {
              kind: "breakdown",
              presentation: "bar",
              by: "products",
              from: "2026-10-01",
              to: "2026-11-01",
              currencies: [
                {
                  currency: "USD",
                  decimals: 2,
                  rows: [
                    { label: "Honey\u0001wax & <tea>", gross: 1200, orders: 1 },
                  ],
                },
              ],
            },
          ],
        },
      ],
      ["control"],
      () => "Products"
    )
    const zip = await JSZip.loadAsync(await reportWorkbook(items, labels))
    const chart = await zip.file("xl/charts/chart1.xml")!.async("string")
    expect(chart).toContain("Honeywax &amp; &lt;tea&gt;")
    expect(chart).not.toContain(String.fromCharCode(1))
    expect(chart).toContain('<c:barDir val="col"/>')
  })

  it("retains tiny exact stock quantities and uses a native chart without a rounded zero format", async () => {
    const items = reportItems(
      [
        {
          id: "stock",
          role: "result",
          text: "",
          views: [
            {
              kind: "stock",
              presentation: "hbar",
              scope: "list",
              total: 1,
              href: "/stock",
              groups: [
                {
                  title: "Main",
                  rows: [
                    {
                      label: "Tea",
                      detail: null,
                      level: "0.00000000001",
                      reserved: "0.0000",
                      available: null,
                      minimum: null,
                      href: null,
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
      ["stock"],
      () => "Stock"
    )
    const bytes = await reportWorkbook(items, labels)
    const zip = await JSZip.loadAsync(bytes)
    const chart = await zip.file("xl/charts/chart1.xml")!.async("string")
    expect(chart).toContain("<c:v>1e-11</c:v>")
    expect(chart).toContain("<c:formatCode>General</c:formatCode>")
    expect(chart).toContain('<c:barDir val="bar"/>')
    const { Workbook } = await import("exceljs")
    const read = new Workbook()
    await read.xlsx.load(bytes as never)
    expect(read.worksheets[1]!.getSheetValues().flat()).toContain(
      "0.00000000001"
    )
    expect(read.worksheets[2]!.getSheetValues().flat()).toContain(
      "0.00000000001"
    )
    expect(read.worksheets[1]!.getSheetValues().flat()).toContainEqual({
      formula: '_xlfn.NUMBERVALUE(E12,".",",")',
      result: 1e-11,
    })
  })

  it("produces a real workbook with literal labels and exact typed values", async () => {
    const items = reportItems(
      [
        {
          id: "result",
          role: "result",
          text: "",
          views: [
            {
              kind: "stock",
              scope: "list",
              total: 20,
              href: "/stock",
              groups: [
                {
                  title: "Main",
                  rows: [
                    {
                      label: "=SUM(A1:A2)",
                      detail: null,
                      level: "9007199254740993.0100",
                      reserved: "0.0000",
                      available: "-0.0001",
                      minimum: null,
                      href: null,
                    },
                  ],
                },
              ],
            },
          ],
        },
      ],
      ["result"],
      () => "Stock"
    )
    const bytes = await reportWorkbook(items, labels)
    const ExcelJS = await import("exceljs")
    const read = new ExcelJS.Workbook()
    await read.xlsx.load(bytes as never)
    expect(read.worksheets).toHaveLength(3)
    const values = read.worksheets[1]!.getSheetValues().flatMap((r) =>
      Array.isArray(r) ? r : []
    )
    expect(values).toContain("=SUM(A1:A2)")
    expect(values).toContain("9007199254740993.0100")
    expect(values).toContain("-0.0001")
    expect(values).not.toContainEqual(
      expect.objectContaining({ formula: expect.anything() })
    )
    expect(read.worksheets.at(-1)!.name).toBe("Raw data")
    expect(read.worksheets.at(-1)!.getSheetValues().flat()).toContain(
      "9007199254740993.0100"
    )
    const zip = await JSZip.loadAsync(bytes)
    const tables = Object.keys(zip.files).filter((path) =>
      /^xl\/tables\/table\d+\.xml$/.test(path)
    )
    for (const table of tables) {
      const xml = await zip.file(table)!.async("string")
      expect(xml).toContain('totalsRowShown="0"')
      expect(xml).not.toContain('totalsRowCount="1"')
    }
  })

  it("styles business amounts and native editable charts without mixing currencies or inventing totals", async () => {
    const items = reportItems(
      [
        {
          id: "totals",
          role: "result",
          text: "",
          views: [
            {
              kind: "sales",
              presentation: "kpi",
              from: "2026-10-01",
              to: "2026-11-01",
              source: "all",
              rows: [
                {
                  currency: "USD",
                  decimals: 2,
                  gross: 12845000,
                  net: 11073276,
                  vat: 1771724,
                  refunded: 125000,
                  orders: 286,
                },
              ],
            },
          ],
        },
        {
          id: "ranking",
          role: "result",
          text: "",
          views: [
            {
              kind: "breakdown",
              presentation: "pie",
              by: "products",
              from: "2026-10-01",
              to: "2026-11-01",
              currencies: [
                {
                  currency: "USD",
                  decimals: 2,
                  rows: [
                    { label: "=Honey & wax", gross: 3845000, orders: 5 },
                    { label: "Coffee", gross: 2750000, orders: 4 },
                  ],
                },
                {
                  currency: "JPY",
                  decimals: 0,
                  rows: [{ label: "Tea", gross: 2100, orders: 2 }],
                },
              ],
            },
          ],
        },
      ],
      ["totals", "ranking"],
      (v) => (v.kind === "sales" ? "Sales" : "Products / 'October'")
    )
    const bytes = await reportWorkbook(items, labels)
    const { Workbook } = await import("exceljs")
    const read = new Workbook()
    await read.xlsx.load(bytes as never)
    expect(read.worksheets).toHaveLength(4)
    const sales = read.worksheets[1]!
    let amount: import("exceljs").Cell | undefined
    sales.eachRow((row) =>
      row.eachCell((cell) => {
        if (cell.value === 128450) amount = cell
      })
    )
    expect(amount?.numFmt).toContain('"USD"')
    expect(amount?.numFmt).toContain("0.00")
    expect(sales.views[0]?.state).toBe("frozen")
    expect(sales.pageSetup.fitToWidth).toBe(1)
    expect(sales.getTables().length).toBeGreaterThan(0)
    expect(read.worksheets[3]!.getSheetValues().flat()).toContain(12845000)
    const summaryValues = read.worksheets[0]!.getSheetValues().flat()
    expect(summaryValues).not.toContain("Average order")
    expect(
      new Set(
        summaryValues.flatMap((value) =>
          typeof value === "object" && value && "formula" in value
            ? [value.formula]
            : []
        )
      ).size
    ).toBe(5)
    const zip = await JSZip.loadAsync(bytes)
    const charts = Object.keys(zip.files).filter((name) =>
      /xl\/charts\/chart\d+\.xml$/.test(name)
    )
    expect(charts).toHaveLength(2)
    const chartXml = await Promise.all(
      charts.map((path) => zip.file(path)!.async("string"))
    )
    expect(chartXml.every((xml) => xml.includes("<c:pieChart>"))).toBe(true)
    expect(chartXml[0]).toContain("=Honey &amp; wax")
    expect(chartXml[0]).toContain("38450")
    expect(chartXml[0]).not.toContain("2100")
    expect(chartXml[1]).toContain("2100")
    expect(chartXml[1]).not.toContain("38450")
    expect(chartXml.every((xml) => xml.includes("<c:f>&apos;2 Products"))).toBe(
      true
    )
    expect(
      await zip.file("xl/worksheets/sheet1.xml")!.async("string")
    ).toContain("<drawing r:id=")
    expect(await zip.file("[Content_Types].xml")!.async("string")).toContain(
      "drawingml.chart+xml"
    )
  })

  it("preserves money beyond Excel's 15-digit limit as exact text in every sheet", async () => {
    const items = reportItems(
      [
        {
          id: "large",
          role: "result",
          text: "",
          views: [
            {
              kind: "sales",
              from: "2026-10-01",
              to: "2026-11-01",
              source: "all",
              rows: [
                {
                  currency: "USD",
                  decimals: 2,
                  gross: 9007199254740991,
                  net: null,
                  vat: null,
                  refunded: null,
                  orders: 0,
                },
              ],
            },
          ],
        },
      ],
      ["large"],
      () => "Sales"
    )
    const bytes = await reportWorkbook(items, { ...labels, locale: "en-US" })
    const { Workbook } = await import("exceljs")
    const read = new Workbook()
    await read.xlsx.load(bytes as never)
    const detail = read.worksheets[1]!.getSheetValues().flat()
    const raw = read.worksheets[2]!.getSheetValues().flat()
    expect(detail).toContain("$90,071,992,547,409.91")
    expect(raw).toContain("9007199254740991")
    expect(raw).not.toContain(9007199254740991)
    expect(read.worksheets[0]!.getCell("B11").value).toEqual({
      formula: "'1 Sales'!C13",
      result: "$90,071,992,547,409.91",
    })
    expect(
      Object.keys((await JSZip.loadAsync(bytes)).files).filter((path) =>
        /charts\/chart\d+\.xml$/.test(path)
      )
    ).toHaveLength(0)
  })

  it("keeps per-view dates, titles, nulls and fractional non-money values", async () => {
    const items = reportItems(
      [
        {
          id: "several",
          role: "result",
          text: "",
          views: [
            {
              kind: "sales_daily",
              presentation: "table",
              from: "2026-09-01",
              to: "2026-09-02",
              time_zone: "Africa/Dar_es_Salaam",
              currencies: [
                {
                  currency: "USD",
                  decimals: 2,
                  days: [
                    { date: "2026-09-01", gross: 12345, net: 10000, orders: 2 },
                  ],
                },
              ],
            },
            {
              kind: "sales_daily",
              from: "2026-10-01",
              to: "2026-10-02",
              time_zone: "Europe/Amsterdam",
              currencies: [
                {
                  currency: "EUR",
                  decimals: 2,
                  days: [
                    { date: "2026-10-01", gross: 23456, net: 20000, orders: 3 },
                  ],
                },
              ],
            },
            {
              kind: "records",
              resource: "products",
              total: 1,
              href: "/products",
              rows: [
                {
                  id: "p",
                  label: "Honey",
                  detail: null,
                  status: null,
                  money: null,
                  stock: 1.5,
                  href: "/products/p",
                },
              ],
            },
          ],
        },
      ],
      ["several"],
      (view) => ("from" in view ? view.from : "Products")
    )
    const bytes = await reportWorkbook(items, labels)
    const { Workbook } = await import("exceljs")
    const read = new Workbook()
    await read.xlsx.load(bytes as never)
    const detail = read.worksheets[1]!
    const values = detail.getSheetValues().flat()
    expect(values).toContain("2026-09-01")
    expect(values).toContain("2026-10-01")
    expect(
      values.some(
        (v) => typeof v === "string" && v.includes("Europe/Amsterdam")
      )
    ).toBe(true)
    expect(
      values.some(
        (v) =>
          v instanceof Date && v.toISOString() === "2026-10-01T00:00:00.000Z"
      )
    ).toBe(true)
    let fractional: import("exceljs").Cell | undefined
    detail.eachRow((row) =>
      row.eachCell((cell) => {
        if (cell.value === 1.5) fractional = cell
      })
    )
    expect(fractional?.value).toBe(1.5)
    expect(fractional?.numFmt ?? "General").toBe("General")
    const zip = await JSZip.loadAsync(bytes)
    const charts = Object.keys(zip.files).filter((path) =>
      /charts\/chart\d+\.xml$/.test(path)
    )
    expect(charts).toHaveLength(1)
    const chart = await zip.file(charts[0]!)!.async("string")
    expect(chart).toContain("<c:lineChart>")
    expect(chart).toContain("2026-10-01")
    expect(chart).toContain("EUR")
    expect(chart).not.toContain("USD")
    expect(values).not.toContain(0)
  })

  it.each(["pie", "donut"] as const)(
    "keeps negative %s data in tables instead of misleading absolute slices",
    async (presentation) => {
      const items = reportItems(
        [
          {
            id: "signed",
            role: "result",
            text: "",
            views: [
              {
                kind: "breakdown",
                presentation,
                by: "products",
                from: "2026-10-01",
                to: "2026-11-01",
                currencies: [
                  {
                    currency: "USD",
                    decimals: 2,
                    rows: [
                      { label: "Returned", gross: -10000, orders: 1 },
                      { label: "Sold", gross: 20000, orders: 2 },
                    ],
                  },
                ],
              },
            ],
          },
        ],
        ["signed"],
        () => "Products"
      )
      const bytes = await reportWorkbook(items, labels)
      const zip = await JSZip.loadAsync(bytes)
      expect(
        Object.keys(zip.files).filter((path) =>
          /charts\/chart\d+\.xml$/.test(path)
        )
      ).toHaveLength(0)
      const { Workbook } = await import("exceljs")
      const read = new Workbook()
      await read.xlsx.load(bytes as never)
      expect(read.worksheets[1]!.getSheetValues().flat()).toContain(-100)
    }
  )
})
