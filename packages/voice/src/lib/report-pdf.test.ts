import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { reportItems, type ReportLabels } from "./report"
import { reportPdf } from "./report-pdf"

const seen = vi.hoisted(() => ({
  options: {} as Record<string, unknown>,
  page: 1,
  pages: 1,
  size: 10,
  text: [] as {
    value: string
    x: number
    y: number
    page: number
    size: number
  }[],
  boxes: [] as {
    page: number
    x: number
    y: number
    width: number
    height: number
  }[],
}))
vi.mock("svg2pdf.js", () => ({}))
vi.mock("jspdf", () => ({
  jsPDF: class {
    constructor(options: Record<string, unknown>) {
      seen.options = options
    }
    internal = {
      pageSize: {
        getWidth: () => (seen.options.orientation === "portrait" ? 210 : 297),
        getHeight: () => (seen.options.orientation === "portrait" ? 297 : 210),
      },
    }
    addFileToVFS() {}
    addFont() {}
    setFont() {
      return this
    }
    setFontSize(size: number) {
      seen.size = size
      return this
    }
    setTextColor() {
      return this
    }
    setFillColor() {
      return this
    }
    setDrawColor() {
      return this
    }
    setLineWidth() {
      return this
    }
    setProperties() {
      return this
    }
    addPage() {
      seen.page++
      seen.pages = Math.max(seen.pages, seen.page)
      return this
    }
    setPage(page: number) {
      seen.page = page
      return this
    }
    getNumberOfPages() {
      return seen.pages
    }
    splitTextToSize(value: string, width: number) {
      const count = Math.max(1, Math.floor(width / (seen.size * 0.18)))
      return String(value).match(new RegExp(`.{1,${count}}`, "gu")) ?? [""]
    }
    getTextWidth(value: string) {
      return value.length * seen.size * 0.18
    }
    text(value: string | string[], x: number, y: number) {
      for (const [i, line] of (Array.isArray(value)
        ? value
        : [value]
      ).entries())
        seen.text.push({
          value: line,
          x,
          y: y + i * seen.size * 0.45,
          page: seen.page,
          size: seen.size,
        })
      return this
    }
    rect(x: number, y: number, width: number, height: number) {
      seen.boxes.push({ page: seen.page, x, y, width, height })
      return this
    }
    roundedRect(x: number, y: number, width: number, height: number) {
      return this.rect(x, y, width, height)
    }
    line() {
      return this
    }
    async svg(
      _svg: unknown,
      box: { x: number; y: number; width: number; height: number }
    ) {
      seen.boxes.push({ page: seen.page, ...box })
      return this
    }
    output() {
      return new Blob(["%PDF-mock"], { type: "application/pdf" })
    }
  },
}))

const labels: ReportLabels = {
  locale: "en-US",
  title: "Business report",
  snapshot: "Selected read-only snapshots; may be partial.",
  captured: "Captured",
  widget: "Result",
  period: "Period",
  columns: (key) => key,
}
const sales = reportItems(
  [
    {
      id: "s",
      role: "result",
      text: "",
      createdAt: "2026-11-13T11:30:00Z",
      views: [
        {
          kind: "sales",
          from: "2026-10-01",
          to: "2026-11-01",
          source: null,
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
  ],
  ["s"],
  () => "Monthly sales"
)

beforeEach(() => {
  Object.assign(seen, {
    options: {},
    page: 1,
    pages: 1,
    size: 10,
    text: [],
    boxes: [],
  })
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({
      ok: true,
      arrayBuffer: async () => new Uint8Array([0, 1]).buffer,
    }))
  )
})
afterEach(() => vi.unstubAllGlobals())

describe("formal PDF report layout", () => {
  it("uses portrait pages, branded executive KPIs and page-count footers", async () => {
    await reportPdf(sales, new Map(), labels)
    expect(seen.options.orientation).toBe("portrait")
    expect(seen.text.map((entry) => entry.value)).toContain("GENESY")
    expect(
      seen.text.some(
        (entry) => entry.value === "$128,450.00" && entry.size >= 14
      )
    ).toBe(true)
    expect(
      seen.text.some(
        (entry) => entry.value === "$110,732.76" && entry.size >= 14
      )
    ).toBe(true)
    for (let page = 1; page <= seen.pages; page++)
      expect(
        seen.text.some(
          (entry) =>
            entry.page === page &&
            entry.value.includes(`${page} / ${seen.pages}`)
        )
      ).toBe(true)
  })

  it("retains a very long table value across pages without drawing through the footer", async () => {
    const long = "record-detail ".repeat(700) + "FINAL RECORD DETAIL"
    const items = [
      {
        row: { id: "records", role: "result" as const, text: "" },
        title: "Records",
        tables: [
          {
            key: "rows",
            columns: ["label", "detail"],
            rows: [
              { label: "ROW-001", detail: long },
              { label: "ROW-002", detail: "Second result" },
            ],
          },
        ],
      },
    ]
    await reportPdf(items, new Map(), labels)
    expect(seen.pages).toBeGreaterThan(2)
    expect(seen.text.map((entry) => entry.value).join("")).toContain(
      "FINAL RECORD DETAIL"
    )
    expect(
      seen.text.filter((entry) => entry.y < 280).map((entry) => entry.value)
    ).toContain("Second result")
    expect(seen.text.every((entry) => entry.y <= 294)).toBe(true)
    expect(
      seen.text
        .filter((entry) =>
          /record-detail|ROW-00|Second result|FINAL RECORD DETAIL/.test(
            entry.value
          )
        )
        .every((entry) => entry.y <= 274)
    ).toBe(true)
    const dataPages = new Set(
      seen.text
        .filter(
          (entry) => entry.value === "ROW-001" || entry.value === "ROW-002"
        )
        .map((entry) => entry.page)
    )
    for (const page of dataPages)
      expect(
        seen.text.some(
          (entry) => entry.page === page && entry.value === "detail"
        )
      ).toBe(true)
  })

  it("starts a table on the next page rather than leaving one data row after a chart", async () => {
    const rows = Array.from({ length: 20 }, (_, i) => ({
      label: `ROW-${String(i + 1).padStart(3, "0")}`,
    }))
    const item = {
      row: { id: "chart", role: "result" as const, text: "" },
      title: "Sales chart",
      tables: [{ key: "rows", columns: ["label"], rows }],
    }
    const svg = {
      getAttribute: () => "Gross sales",
      viewBox: { baseVal: { width: 720, height: 720 } },
    } as unknown as SVGSVGElement
    await reportPdf([item], new Map([["chart", [svg]]]), labels)
    const chart = seen.boxes.find((box) => box.height > 100)!
    expect(chart).toBeDefined()
    expect(
      seen.text.filter(
        (entry) => entry.page === chart.page && /^ROW-/.test(entry.value)
      )
    ).toHaveLength(0)
    expect(seen.text.filter((entry) => /^ROW-/.test(entry.value))).toHaveLength(
      20
    )
  })

  it("keeps legacy compound views under their own titles and periods", async () => {
    const first = sales[0]!.row.views![0] as Extract<
      import("./views").ResultView,
      { kind: "sales" }
    >
    const items = reportItems(
      [
        {
          id: "multi",
          role: "result",
          text: "",
          views: [
            first,
            {
              ...first,
              from: "2026-12-01",
              to: "2027-01-01",
              rows: [{ ...first.rows[0]!, gross: 22222 }],
            },
          ],
        },
      ],
      ["multi"],
      (view) =>
        "from" in view && view.from === "2026-12-01"
          ? "December sales"
          : "October sales"
    )
    await reportPdf(items, new Map(), labels)
    const detail = seen.text.findIndex(
      (entry) => entry.value === "December sales" && entry.size >= 12
    )
    expect(detail).toBeGreaterThan(0)
    expect(
      seen.text
        .slice(detail)
        .some((entry) => entry.value.includes("2026-12-01"))
    ).toBe(true)
    expect(
      seen.text
        .slice(detail)
        .some((entry) => entry.value === "$222.22" && entry.size === 8)
    ).toBe(true)
  })

  it("propagates font loading errors instead of downloading a broken report", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: false }))
    )
    await expect(reportPdf(sales, new Map(), labels)).rejects.toThrow(
      "Report font could not load"
    )
  })
})
