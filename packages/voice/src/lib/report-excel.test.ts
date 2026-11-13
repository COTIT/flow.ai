import { describe, expect, it } from "vitest"
import { reportItems, type ReportLabels } from "./report"
import { reportWorkbook } from "./report-excel"

const labels: ReportLabels = {
  title: "Report",
  snapshot: "Displayed snapshot; may be partial.",
  captured: "Captured",
  widget: "Widget",
  period: "Period",
  columns: (key) => key,
}

describe("reportWorkbook", () => {
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
    expect(read.worksheets).toHaveLength(2)
    const values = read.worksheets[1]!.getSheetValues().flatMap((r) =>
      Array.isArray(r) ? r : []
    )
    expect(values).toContain("=SUM(A1:A2)")
    expect(values).toContain("9007199254740993.0100")
    expect(values).toContain("-0.0001")
    expect(values).not.toContainEqual(
      expect.objectContaining({ formula: expect.anything() })
    )
  })
})
