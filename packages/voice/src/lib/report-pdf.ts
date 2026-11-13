import type { jsPDF as Pdf } from "jspdf"
import {
  reportPeriod,
  type ReportCell,
  type ReportItem,
  type ReportLabels,
} from "./report"

const asText = (value: ReportCell | undefined) =>
  value === null || value === undefined ? "—" : String(value)

async function font(
  pdf: Pdf,
  weight: "Regular" | "SemiBold",
  style: "normal" | "bold"
) {
  const response = await fetch(`/report-fonts/Poppins-${weight}.ttf`)
  if (!response.ok) throw new Error("Report font could not load")
  const bytes = new Uint8Array(await response.arrayBuffer())
  let binary = ""
  for (let i = 0; i < bytes.length; i += 16384)
    binary += String.fromCharCode(...bytes.subarray(i, i + 16384))
  pdf.addFileToVFS(`Poppins-${weight}.ttf`, btoa(binary))
  pdf.addFont(`Poppins-${weight}.ttf`, "Poppins", style)
}

/** Curated chart SVG only; tables use exact snapshot fields, with repeated headers after breaks. */
export async function reportPdf(
  items: ReportItem[],
  charts: ReadonlyMap<string, SVGSVGElement[]>,
  labels: ReportLabels
): Promise<Blob> {
  const [{ jsPDF }] = await Promise.all([import("jspdf"), import("svg2pdf.js")])
  const pdf = new jsPDF({ orientation: "landscape", unit: "mm", format: "a4" })
  await font(pdf, "Regular", "normal")
  await font(pdf, "SemiBold", "bold")
  const margin = 14
  const width = pdf.internal.pageSize.getWidth() - margin * 2
  const bottom = pdf.internal.pageSize.getHeight() - margin
  let y = margin
  const page = () => {
    pdf.addPage()
    y = margin
  }
  const text = (value: string, bold = false, size = 10) => {
    pdf.setFont("Poppins", bold ? "bold" : "normal")
    pdf.setFontSize(size)
    const lines = pdf.splitTextToSize(value, width) as string[]
    for (const line of lines) {
      if (y + 6 > bottom) page()
      pdf.text(line, margin, y + 4)
      y += 6
    }
  }
  text(labels.title, true, 16)
  text(labels.snapshot, false, 9)
  for (const [index, item] of items.entries()) {
    if (index) page()
    text(item.title, true, 13)
    const period = reportPeriod(item.row)
    if (period) text(`${labels.period}: ${period}`, false, 9)
    if (item.row.createdAt)
      text(`${labels.captured}: ${item.row.createdAt}`, false, 9)
    for (const svg of charts.get(item.row.id) ?? []) {
      const caption = svg.getAttribute("aria-label")
      if (caption) text(caption, false, 9)
      const box = svg.viewBox.baseVal
      const height = width * ((box.height || 320) / (box.width || 720))
      if (y + height > bottom) page()
      await pdf.svg(svg, { x: margin, y, width, height })
      y += height + 6
    }
    for (const table of item.tables) {
      // Wide tables split by columns; first column repeats to keep rows identifiable.
      for (let start = 0; start < table.columns.length; start += 5) {
        const columns = start
          ? [table.columns[0]!, ...table.columns.slice(start, start + 5)]
          : table.columns.slice(0, 5)
        const colWidth = width / columns.length
        const drawRow = (values: string[], bold: boolean) => {
          pdf.setFont("Poppins", bold ? "bold" : "normal")
          pdf.setFontSize(8)
          const lines = values.map(
            (value) => pdf.splitTextToSize(value, colWidth - 3) as string[]
          )
          const height =
            Math.max(...lines.map((value) => value.length), 1) * 4.2 + 3
          pdf.setDrawColor(210)
          if (bold) {
            pdf.setFillColor("#f0f0f0")
            pdf.rect(margin, y, width, height, "F")
          }
          lines.forEach((value, i) =>
            pdf.text(value, margin + i * colWidth + 1, y + 4)
          )
          pdf.line(margin, y + height, margin + width, y + height)
          y += height
        }
        const headers = columns.map(labels.columns)
        if (y + 20 > bottom) page()
        drawRow(headers, true)
        for (const row of table.rows) {
          const values = columns.map((column) => asText(row[column]))
          pdf.setFontSize(8)
          const rowHeight =
            Math.max(
              ...values.map(
                (value) =>
                  (pdf.splitTextToSize(value, colWidth - 3) as string[]).length
              ),
              1
            ) *
              4.2 +
            3
          if (y + rowHeight > bottom) {
            page()
            drawRow(headers, true)
          }
          drawRow(values, false)
        }
        y += 5
      }
    }
  }
  return pdf.output("blob")
}
