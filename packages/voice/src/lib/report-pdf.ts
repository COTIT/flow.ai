import type { jsPDF as Pdf } from "jspdf"
import {
  reportPeriod,
  reportSummary,
  reportTables,
  pdfReportTables,
  fitReportChart,
  type ReportCell,
  type ReportItem,
  type ReportLabels,
  type ReportTable,
} from "./report"

const COLORS = {
  ink: "#252622",
  gold: "#C5A15B",
  paper: "#FAF9F6",
  muted: "#6D7067",
  line: "#E2E3DC",
  soft: "#F2F1EB",
}
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

/** Formal report from selected snapshots only; financial figures remain the API's values. */
export async function reportPdf(
  items: ReportItem[],
  charts: ReadonlyMap<string, SVGSVGElement[]>,
  labels: ReportLabels
): Promise<Blob> {
  const [{ jsPDF }] = await Promise.all([import("jspdf"), import("svg2pdf.js")])
  const pdf = new jsPDF({ orientation: "portrait", unit: "mm", format: "a4" })
  await font(pdf, "Regular", "normal")
  await font(pdf, "SemiBold", "bold")
  pdf.setProperties({
    title: labels.title,
    author: labels.brand ?? "Genesy",
    subject: labels.snapshot,
  })
  const margin = 15
  const width = pdf.internal.pageSize.getWidth() - margin * 2
  const bottom = 274
  const locale = labels.locale ?? "en"
  const pages: { section: string; source: string; captured: string }[] = []
  let y = 0
  let context = {
    section: labels.summary ?? labels.title,
    source: labels.title,
    captured: "",
  }
  const setText = (size = 9, bold = false, color = COLORS.ink) => {
    pdf.setFont("Poppins", bold ? "bold" : "normal")
    pdf.setFontSize(size)
    pdf.setTextColor(color)
  }
  const short = (value: string, available: number) => {
    const lines = pdf.splitTextToSize(value, available) as string[]
    return lines.length > 1 ? `${lines[0]}...` : (lines[0] ?? "")
  }
  const date = (value: string) => {
    const parsed = new Date(value)
    return Number.isNaN(parsed.getTime())
      ? value
      : new Intl.DateTimeFormat(locale, {
          dateStyle: "medium",
          timeStyle: "short",
          timeZone: "UTC",
        }).format(parsed) + " UTC"
  }
  const page = (next = context) => {
    if (pages.length) pdf.addPage()
    context = next
    pages.push(context)
    setText(14, true)
    pdf.text(labels.brand ?? "GENESY", margin, 19)
    if (labels.businessName) {
      setText(8, false, COLORS.muted)
      pdf.text(short(labels.businessName, width / 2), margin + width, 18, {
        align: "right",
      })
    }
    pdf.setFillColor(COLORS.gold)
    pdf.rect(margin, 24, 13, 0.7, "F")
    pdf.setDrawColor(COLORS.line)
    pdf.setLineWidth(0.25)
    pdf.line(margin, 31, margin + width, 31)
    setText(8, true, COLORS.gold)
    pdf.text(short(context.section, width), margin, 38)
    y = 47
  }
  const ensure = (height: number) => {
    if (y + height > bottom) page()
  }
  const text = (
    value: string,
    size = 9,
    bold = false,
    color = COLORS.ink,
    available = width,
    x = margin
  ) => {
    setText(size, bold, color)
    const lines = pdf.splitTextToSize(value, available) as string[]
    const leading = size * 0.3528 * 1.45
    for (const line of lines) {
      ensure(leading)
      setText(size, bold, color)
      pdf.text(line, x, y + size * 0.3528)
      y += leading
    }
  }
  const heading = (value: string, size = 13) => {
    ensure(14)
    text(value, size, true)
    y += 3
  }
  const metadata = (name: string, value: string) =>
    text(`${name}: ${value}`, 8, false, COLORS.muted)
  const label = (key: string) => {
    const value = labels.columns(
      key.endsWith(".minor") ? key.slice(0, -6) : key
    )
    return labels.minorUnits
      ? value.replace(` (${labels.minorUnits})`, "")
      : value
  }

  page()
  text(labels.title, 25, true)
  y += 3
  text(labels.snapshot, 9, false, COLORS.muted)
  y += 8
  const summaries = reportSummary(items, locale)
  if (summaries.length) {
    heading(labels.summary ?? labels.title)
    for (const summary of summaries) {
      ensure(37)
      text(summary.title, 10, true)
      metadata(labels.period, summary.period)
      metadata(labels.currency ?? label("currency"), summary.currency)
      if (summary.captured) metadata(labels.captured, date(summary.captured))
      y += 4
      for (let start = 0; start < summary.metrics.length; start += 3) {
        ensure(33)
        const metrics = summary.metrics.slice(start, start + 3)
        const cardWidth = (width - 8) / 3
        for (const [index, metric] of metrics.entries()) {
          const x = margin + index * (cardWidth + 4)
          pdf.setFillColor(COLORS.paper)
          pdf.roundedRect(x, y, cardWidth, 29, 2, 2, "F")
          setText(7.5, false, COLORS.muted)
          pdf.text(
            pdf.splitTextToSize(label(metric.key), cardWidth - 10),
            x + 5,
            y + 7
          )
          let size = 18
          setText(size, true)
          while (pdf.getTextWidth(metric.value) > cardWidth - 10 && size > 9) {
            size--
            setText(size, true)
          }
          pdf.text(metric.value, x + 5, y + 22)
        }
        y += 33
      }
      y += 6
    }
  }
  heading(labels.selectedResults ?? labels.details ?? labels.widget)
  for (const [index, item] of items.entries()) {
    ensure(20)
    text(`${String(index + 1).padStart(2, "0")}  ${item.title}`, 10, true)
    const periods = [
      ...new Set(
        (item.row.views ?? [])
          .map((view) => reportPeriod({ ...item.row, views: [view] }))
          .filter(Boolean)
      ),
    ]
    if (periods.length) metadata(labels.period, periods.join(" | "))
    if (item.row.createdAt) metadata(labels.captured, date(item.row.createdAt))
    y += 5
  }

  const drawTable = (table: ReportTable) => {
    if (!table.columns.length || !table.rows.length) return
    // Fit business columns first; panels retain an ordinal for matching repeated labels.
    const panels: { columns: string[]; weights: number[] }[] = []
    for (const column of table.columns) {
      const key = column.split(".").at(-1)
      const minimum =
        key === "currency"
          ? 17
          : ["orders", "count", "minimum", "share", "change_bp"].includes(
                key ?? ""
              )
            ? 20
            : ["label", "title", "detail", "href"].includes(key ?? "")
              ? 48
              : 29
      let panel = panels.at(-1)
      if (
        !panel ||
        panel.weights.reduce((sum, value) => sum + value, 0) + minimum >
          width - 10
      ) {
        panel = { columns: [], weights: [] }
        panels.push(panel)
      }
      panel.columns.push(column)
      panel.weights.push(minimum)
    }
    for (const [part, panel] of panels.entries()) {
      const columns = panel.columns
      const total = panel.weights.reduce((sum, value) => sum + value, 0)
      const cellWidths = [
        10,
        ...panel.weights.map((weight) => ((width - 10) * weight) / total),
      ]
      const headers = ["#", ...columns.map(label)]
      const rightAligned = [
        false,
        ...columns.map(
          (column) =>
            column.endsWith(".minor") ||
            table.rows.some(
              (row) =>
                typeof row[column] === "number" ||
                (typeof row[column] === "string" &&
                  /^-?\d+(?:\.\d+)?$/.test(row[column] as string))
            ) ||
            ["gross", "net", "vat", "refunded", "current", "base"].includes(
              column
            )
        ),
      ]
      const lineHeight = 4.1
      const padding = 2.2
      const linesOf = (values: string[], bold: boolean) => {
        setText(8, bold)
        return values.map(
          (value, index) =>
            pdf.splitTextToSize(value, cellWidths[index]! - 4) as string[]
        )
      }
      const headerLines = linesOf(headers, true)
      const headerHeight =
        Math.max(...headerLines.map((cell) => cell.length), 1) * lineHeight +
        padding * 2
      const rows = table.rows.map((row, index) =>
        linesOf(
          [
            String(index + 1).padStart(2, "0"),
            ...columns.map((column) => asText(row[column])),
          ],
          false
        )
      )
      const heightOf = (lines: string[][]) =>
        Math.max(...lines.map((cell) => cell.length), 1) * lineHeight +
        padding * 2
      const minRows = rows
        .slice(0, 2)
        .reduce((sum, row) => sum + heightOf(row), 0)
      ensure(Math.min(bottom - 47, 13 + headerHeight + minRows))
      heading(
        labels.sections?.(table.key) ?? labels.details ?? labels.widget,
        12
      )
      if (part) {
        text(
          `${labels.continued ?? labels.widget} ${part + 1}`,
          8,
          false,
          COLORS.muted
        )
        y += 2
      }
      const draw = (lines: string[][], bold: boolean, rowNumber = 0) => {
        const height = heightOf(lines)
        if (bold || rowNumber % 2) {
          pdf.setFillColor(bold ? COLORS.soft : COLORS.paper)
          pdf.rect(margin, y, width, height, "F")
        }
        setText(8, bold, bold ? COLORS.muted : COLORS.ink)
        let x = margin
        for (const [index, cell] of lines.entries()) {
          cell.forEach((line, offset) =>
            pdf.text(
              line,
              rightAligned[index] ? x + cellWidths[index]! - 2 : x + 2,
              y + padding + 2.8 + offset * lineHeight,
              { align: rightAligned[index] ? "right" : "left" }
            )
          )
          x += cellWidths[index]!
        }
        pdf.setDrawColor(COLORS.line)
        pdf.setLineWidth(0.2)
        pdf.line(margin, y + height, margin + width, y + height)
        y += height
      }
      const tablePage = () => {
        page()
        text(labels.continued ?? context.section, 8, false, COLORS.muted)
        y += 3
        draw(headerLines, true)
      }
      draw(headerLines, true)
      let index = 0
      while (index < rows.length) {
        const available = bottom - y
        const current = rows[index]!
        const fullPage = bottom - 55 - headerHeight
        if (heightOf(current) > fullPage) {
          let offset = 0
          const count = Math.max(...current.map((cell) => cell.length))
          while (offset < count) {
            const fit = Math.floor((bottom - y - padding * 2) / lineHeight)
            if (fit < 1) {
              tablePage()
              continue
            }
            const fragment = current.map((cell, col) =>
              col === 0 ? cell : cell.slice(offset, offset + fit)
            )
            draw(fragment, false, index)
            offset += fit
            if (offset < count) tablePage()
          }
          index++
          continue
        }
        let fit = 0
        let used = 0
        while (
          index + fit < rows.length &&
          used + heightOf(rows[index + fit]!) <= available
        )
          used += heightOf(rows[index + fit++]!)
        const remaining = rows.length - index
        if (remaining - fit === 1 && fit > 2) fit--
        if (
          !fit ||
          (fit === 1 &&
            remaining > 1 &&
            heightOf(current) + heightOf(rows[index + 1]!) <= fullPage)
        ) {
          tablePage()
          continue
        }
        for (let end = index + fit; index < end; index++)
          draw(rows[index]!, false, index)
        if (index < rows.length) tablePage()
      }
      y += 7
    }
  }

  const showMetadata = (tables: ReportTable[]) => {
    for (const table of tables.filter((table) => table.key === "metadata")) {
      for (const row of table.rows) {
        const fields = table.columns.filter(
          (key) => !["kind", "presentation", "href", "from", "to"].includes(key)
        )
        if (fields.length)
          text(
            fields
              .map((key) => `${label(key)}: ${asText(row[key])}`)
              .join(" · "),
            8,
            false,
            COLORS.muted
          )
      }
    }
  }

  for (const [index, item] of items.entries()) {
    page({
      section: `${String(index + 1).padStart(2, "0")}  ${labels.details ?? labels.widget}`,
      source: item.title,
      captured: item.row.createdAt ? date(item.row.createdAt) : "",
    })
    text(item.title, 22, true)
    y += 3
    const multiple = (item.row.views?.length ?? 0) > 1
    const sections = multiple
      ? item.row.views!.map((view, viewIndex) => ({
          title: item.viewTitles?.[viewIndex] ?? item.title,
          period: reportPeriod({ ...item.row, views: [view] }),
          tables: reportTables(view),
        }))
      : [
          {
            title: item.title,
            period: reportPeriod(item.row),
            tables: item.tables,
          },
        ]
    if (item.row.createdAt) metadata(labels.captured, date(item.row.createdAt))
    for (const section of sections) {
      if (multiple) text(section.title, 10, true)
      if (section.period) metadata(labels.period, section.period)
      if (!multiple) showMetadata(section.tables)
    }
    y += 6
    for (const svg of charts.get(item.row.id) ?? []) {
      const caption =
        svg.getAttribute("aria-label") ?? labels.chartTitle ?? item.title
      setText(9, true)
      const captionLines = pdf.splitTextToSize(caption, width) as string[]
      const captionHeight = captionLines.length * 4.7 + 3
      const box = svg.viewBox.baseVal
      const size = fitReportChart(
        box.width || 720,
        box.height || 320,
        width,
        bottom - 47 - captionHeight - 6
      )
      ensure(captionHeight + size.height + 6)
      text(caption, 9, true)
      y += 3
      await pdf.svg(svg, { x: margin + (width - size.width) / 2, y, ...size })
      y += size.height + 8
    }
    for (const section of sections) {
      if (multiple) {
        heading(section.title, 12)
        if (section.period) metadata(labels.period, section.period)
        showMetadata(section.tables)
        y += 3
      }
      for (const table of pdfReportTables(section.tables, locale))
        drawTable(table)
    }
  }

  for (let index = 0; index < pages.length; index++) {
    pdf.setPage(index + 1)
    const meta = pages[index]!
    pdf.setDrawColor(COLORS.line)
    pdf.setLineWidth(0.25)
    pdf.line(margin, 281, margin + width, 281)
    setText(6.7, false, COLORS.muted)
    pdf.text(short(labels.snapshot, width - 25), margin, 287)
    setText(8, true)
    pdf.text(
      labels.page?.(index + 1, pages.length) ??
        `${index + 1} / ${pages.length}`,
      margin + width,
      287,
      { align: "right" }
    )
    setText(6.5, false, COLORS.muted)
    pdf.text(
      short(
        `${labels.source ?? labels.widget}: ${meta.source}${meta.captured ? ` | ${labels.captured}: ${meta.captured}` : ""}`,
        width
      ),
      margin,
      293
    )
  }
  return pdf.output("blob")
}
