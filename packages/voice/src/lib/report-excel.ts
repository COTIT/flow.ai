import { reportPeriod, type ReportItem, type ReportLabels } from "./report"

/** Browser-only XLSX generation; decimal strings and formula-looking labels remain literal text. */
export async function reportWorkbook(
  items: ReportItem[],
  labels: ReportLabels
): Promise<Uint8Array> {
  const { Workbook } = await import("exceljs")
  const workbook = new Workbook()
  const summary = workbook.addWorksheet(
    labels.title.slice(0, 31).replace(/[\\/*?:[\]]/g, " ") || "Report"
  )
  summary.addRow([labels.title])
  summary.addRow([labels.snapshot])
  summary.addRow([labels.widget, labels.period, labels.captured])
  summary.columns = [{ width: 44 }, { width: 28 }, { width: 28 }]
  for (const [index, item] of items.entries()) {
    summary.addRow([
      item.title,
      reportPeriod(item.row),
      item.row.createdAt ?? null,
    ])
    // Index makes duplicate, long and invalid worksheet titles unambiguous.
    const name = `${index + 1} ${item.title}`
      .replace(/[\\/*?:[\]]/g, " ")
      .slice(0, 31)
      .replace(/'+$/g, "")
    const sheet = workbook.addWorksheet(name)
    sheet.views = [{ state: "frozen", ySplit: 3 }]
    sheet.addRow([item.title])
    sheet.addRow([labels.snapshot])
    sheet.addRow([labels.captured, item.row.createdAt ?? null])
    for (const table of item.tables) {
      sheet.addRow([])
      const heading = sheet.addRow(table.columns.map(labels.columns))
      heading.font = { bold: true }
      for (const row of table.rows) {
        const added = sheet.addRow(
          table.columns.map((column) => row[column] ?? null)
        )
        added.eachCell((cell) => {
          if (typeof cell.value === "string") cell.numFmt = "@"
          else if (typeof cell.value === "number") cell.numFmt = "0"
        })
      }
    }
    sheet.columns.forEach((column) => {
      column.width = 24
    })
  }
  return new Uint8Array(await workbook.xlsx.writeBuffer())
}
