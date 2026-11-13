import type { Cell, Workbook, Worksheet } from "exceljs"
import { formatMoney } from "@genesy/ui/lib/money"
import {
  reportPeriod,
  reportSummary,
  reportTables,
  type ReportCell,
  type ReportItem,
  type ReportLabels,
  type ReportTable,
} from "./report"
import { addWorkbookCharts, type WorkbookChart } from "./report-excel-charts"
import { drawnAs, type ResultView } from "./views"

const INK = "252622",
  GOLD = "C5A15B",
  PAPER = "FAF9F6",
  MUTED = "77786D",
  LINE = "E5E2DA"
const PLOTS = new Set(["line", "sparkline", "bar", "hbar", "pie", "donut"])
const MONEY = new Set(["gross", "net", "vat", "refunded", "current", "base"])
const precise = (value: number) =>
  Number.isFinite(value) &&
  Math.abs(value)
    .toString()
    .replace(/[.e+-]/g, "")
    .replace(/^0+|0+$/g, "").length <= 15
const sourceValue = (value: ReportCell): ReportCell =>
  typeof value === "number" && !precise(value) ? String(value) : value
const quoteSheet = (name: string) => `'${name.replaceAll("'", "''")}'`
const moneyFormat = (currency: string, decimals: number) =>
  `"${currency.replaceAll('"', '""')}" #,##0${decimals ? `.${"0".repeat(decimals)}` : ""};[Red]("${currency.replaceAll('"', '""')}" #,##0${decimals ? `.${"0".repeat(decimals)}` : ""})`

function money(row: Record<string, ReportCell>, column: string) {
  if (!column.endsWith(".minor") && !MONEY.has(column)) return null
  const prefix = column.endsWith(".minor") ? column.slice(0, -5) : ""
  const currency = row[`${prefix}currency`],
    decimals = row[`${prefix}decimals`]
  return typeof currency === "string" && typeof decimals === "number"
    ? { currency, decimals }
    : null
}

function heading(labels: ReportLabels, key: string) {
  let result = labels.columns(key.endsWith(".minor") ? key.slice(0, -6) : key)
  if (labels.minorUnits) result = result.replace(` (${labels.minorUnits})`, "")
  return result
}

function name(workbook: Workbook, title: string, fallback: string) {
  const clean =
    // eslint-disable-next-line no-control-regex -- Worksheet names cannot contain control characters.
    title.replace(/[\\/*?:[\]'\u0000-\u001f]/g, " ").trim() || fallback
  let result = clean.slice(0, 31).replace(/'+$/g, "")
  let suffix = 1
  while (workbook.getWorksheet(result))
    result = `${clean.slice(0, 25)} (${suffix++})`
      .slice(0, 31)
      .replace(/'+$/g, "")
  return result
}

function text(
  sheet: Worksheet,
  row: number,
  from: number,
  to: number,
  value: string,
  size = 11,
  bold = false,
  color = INK
) {
  if (from !== to) sheet.mergeCells(row, from, row, to)
  const cell = sheet.getCell(row, from)
  cell.value = value || null
  cell.font = { name: "Poppins", size, bold, color: { argb: color } }
  cell.alignment = { vertical: "middle", wrapText: true }
  return cell
}

function setup(
  sheet: Worksheet,
  labels: ReportLabels,
  title: string,
  context: string,
  end = 8
) {
  sheet.properties.defaultRowHeight = 21
  sheet.views = [{ state: "frozen", ySplit: 8, showGridLines: false }]
  sheet.pageSetup = {
    paperSize: 9,
    orientation: "landscape",
    fitToPage: true,
    fitToWidth: 1,
    fitToHeight: 0,
    printTitlesRow: "1:8",
    margins: {
      left: 0.35,
      right: 0.35,
      top: 0.4,
      bottom: 0.4,
      header: 0.2,
      footer: 0.2,
    },
  }
  sheet.headerFooter = {
    oddFooter: `&L${(labels.brand ?? "Genesy").replaceAll("&", "&&")}&R&P / &N`,
  }
  sheet.getColumn(1).width = 3
  for (let col = 2; col <= end; col++) sheet.getColumn(col).width = 22
  text(
    sheet,
    2,
    2,
    end,
    (labels.brand ?? "Genesy").toUpperCase(),
    10,
    true,
    GOLD
  )
  text(sheet, 3, 2, end, title, 22, true)
  sheet.getRow(3).height = 38
  text(sheet, 5, 2, end, labels.businessName ?? "", 11, true)
  text(sheet, 6, 2, end, context, 10, false, MUTED)
  text(sheet, 7, 2, end, labels.snapshot, 10, false, MUTED)
  sheet.getRow(7).height = 34
  for (let col = 2; col <= end; col++)
    sheet.getCell(4, col).border = {
      bottom: { style: "thin", color: { argb: GOLD } },
    }
}

type PlacedTable = { table: ReportTable; cells: Map<string, Cell>[] }
type ChartData = Omit<WorkbookChart, "col" | "row" | "width" | "height">

/** Browser-only export: source text stays literal, including formula-looking labels and decimal quantities. */
export async function reportWorkbook(
  items: ReportItem[],
  labels: ReportLabels
): Promise<Uint8Array> {
  const { Workbook } = await import("exceljs")
  const workbook = new Workbook()
  workbook.creator = labels.brand ?? "Genesy"
  workbook.title = labels.title
  const summary = workbook.addWorksheet(
    name(workbook, labels.summary ?? "Summary", "Summary")
  )
  setup(
    summary,
    labels,
    labels.title,
    labels.selectedResults ?? labels.widget,
    15
  )
  for (let col = 2; col <= 15; col++) summary.getColumn(col).width = 10
  summary.views = [{ state: "normal", showGridLines: false }]
  const details = items.map((item, index) =>
    workbook.addWorksheet(
      name(
        workbook,
        `${index + 1} ${item.title}`,
        `${index + 1} ${labels.data ?? "Data"}`
      )
    )
  )
  const raw = workbook.addWorksheet(
    name(workbook, labels.rawData ?? "Raw data", "Raw data")
  )
  setup(raw, labels, labels.rawData ?? "Raw data", labels.snapshot, 10)
  const charts: ChartData[] = []
  const metricCells = new Map<string, Cell>()
  let tableId = 0,
    rawRow = 9

  function writeTable(
    sheet: Worksheet,
    table: ReportTable,
    start: number,
    original: boolean
  ): PlacedTable {
    const columns = original
      ? table.columns
      : table.columns.filter(
          (column) => column !== "decimals" && !column.endsWith(".decimals")
        )
    if (!columns.length) return { table, cells: [] }
    const seen = new Map<string, number>()
    const headers = columns.map((key) => {
      const label =
        (original ? labels.columns(key) : heading(labels, key)) || key
      const count = (seen.get(label.toLowerCase()) ?? 0) + 1
      seen.set(label.toLowerCase(), count)
      return count > 1 ? `${label} (${count})` : label
    })
    const rows = table.rows.map((row) =>
      columns.map((column) => {
        const value = row[column] ?? null
        const unit = money(row, column)
        if (!original && unit && typeof value === "number") {
          const amount = value / 10 ** unit.decimals
          return precise(value) &&
            precise(amount) &&
            Math.round(amount * 10 ** unit.decimals) === value
            ? amount
            : formatMoney(
                value,
                unit.decimals,
                unit.currency,
                labels.locale ?? "en"
              )
        }
        return sourceValue(value)
      })
    )
    sheet.addTable({
      name: `GenesyData${++tableId}`,
      ref: `B${start}`,
      headerRow: true,
      totalsRow: false,
      style: { theme: "TableStyleLight9", showRowStripes: false },
      columns: headers.map((header) => ({ name: header, filterButton: true })),
      rows,
    })
    columns.forEach((column, i) => {
      const cell = sheet.getCell(start, i + 2)
      cell.font = {
        name: "Poppins",
        size: 10,
        bold: true,
        color: { argb: "FFFFFF" },
      }
      cell.fill = { type: "pattern", pattern: "solid", fgColor: { argb: INK } }
      cell.alignment = { wrapText: true, vertical: "middle" }
      const width = ["label", "detail", "name", "title"].includes(column)
        ? 34
        : 23
      sheet.getColumn(i + 2).width = Math.max(
        sheet.getColumn(i + 2).width ?? 0,
        width
      )
    })
    sheet.getRow(start).height = 34
    const placed = table.rows.map(
      (row, index) =>
        new Map(
          columns.map((column, i) => {
            const cell = sheet.getCell(start + index + 1, i + 2)
            cell.font = { name: "Poppins", size: 10, color: { argb: INK } }
            cell.alignment = { vertical: "top", wrapText: true }
            cell.fill = {
              type: "pattern",
              pattern: "solid",
              fgColor: { argb: index % 2 ? PAPER : "FFFFFF" },
            }
            cell.border = { bottom: { style: "hair", color: { argb: LINE } } }
            const unit = money(row, column)
            cell.numFmt =
              typeof cell.value === "string"
                ? "@"
                : !original && unit
                  ? moneyFormat(unit.currency, unit.decimals)
                  : typeof cell.value === "number" &&
                      !Number.isInteger(cell.value)
                    ? "General"
                    : "#,##0"
            if (
              !original &&
              typeof cell.value === "string" &&
              /^(date|at|opened|closed|.*\.date)$/.test(column) &&
              /^\d{4}-\d{2}-\d{2}$/.test(cell.value)
            ) {
              cell.value = new Date(`${cell.value}T00:00:00Z`)
              cell.numFmt = "dd-mmm-yyyy"
            }
            if (typeof cell.value === "string")
              sheet.getRow(start + index + 1).height = Math.max(
                sheet.getRow(start + index + 1).height ?? 21,
                Math.min(
                  409,
                  16 *
                    Math.ceil(
                      cell.value.length / (sheet.getColumn(i + 2).width ?? 23)
                    )
                )
              )
            return [column, cell]
          })
        )
    )
    return { table, cells: placed }
  }

  for (const [index, item] of items.entries()) {
    const sheet = details[index]!
    const views = item.row.views ?? []
    const maxCols = Math.max(
      8,
      ...item.tables.map((table) => table.columns.length + 1)
    )
    setup(
      sheet,
      labels,
      item.title,
      `${reportPeriod(item.row)}${item.row.createdAt ? ` · ${labels.captured}: ${item.row.createdAt}` : ""}`,
      maxCols
    )
    if (maxCols > 9) {
      sheet.pageSetup.fitToWidth = 0
    }
    let row = 9,
      chartCol = maxCols + 2

    function plot(
      view: ResultView,
      title: string,
      unit: string,
      categoryCells: Cell[],
      valueCells: Cell[],
      values?: (number | null)[],
      format?: string
    ) {
      if (!PLOTS.has(drawnAs(view)) || !categoryCells.length) return
      const numeric =
        values ??
        valueCells.map((cell) =>
          typeof cell.value === "number" ? cell.value : null
        )
      // Native Excel charts cannot faithfully represent more than 15 significant digits.
      if (
        !numeric.some((value) => value !== null) ||
        numeric.some(
          (value, i) =>
            (value !== null && !precise(value)) ||
            (valueCells[i]?.value !== null && value === null)
        )
      )
        return
      if (
        ["pie", "donut"].includes(drawnAs(view)) &&
        numeric.some((value) => value !== null && value < 0)
      )
        return
      const categories = categoryCells.map((cell) =>
        cell.value instanceof Date
          ? cell.value.toISOString().slice(0, 10)
          : String(cell.value ?? "—")
      )
      const categoryRef = `${quoteSheet(sheet.name)}!$${sheet.getColumn(chartCol).letter}$2:$${sheet.getColumn(chartCol).letter}$${categories.length + 1}`
      const valueRef = `${quoteSheet(sheet.name)}!$${sheet.getColumn(chartCol + 1).letter}$2:$${sheet.getColumn(chartCol + 1).letter}$${categories.length + 1}`
      categories.forEach((category, i) => {
        const categoryCell = categoryCells[i]!,
          valueCell = valueCells[i]!
        sheet.getCell(i + 2, chartCol).value = {
          formula:
            categoryCell.value instanceof Date
              ? `TEXT(${categoryCell.address},"yyyy-mm-dd")`
              : categoryCell.address,
          result: category,
        }
        sheet.getCell(i + 2, chartCol + 1).value =
          numeric[i] === null
            ? null
            : {
                formula:
                  typeof valueCell.value === "string"
                    ? `_xlfn.NUMBERVALUE(${valueCell.address},".",",")`
                    : valueCell.address,
                result: numeric[i],
              }
      })
      sheet.getColumn(chartCol).hidden = true
      sheet.getColumn(chartCol + 1).hidden = true
      chartCol += 2
      charts.push({
        title: `${title}${unit ? ` · ${unit}` : ""}${reportPeriod({ ...item.row, views: [view] }) ? ` · ${reportPeriod({ ...item.row, views: [view] })}` : ""}`,
        kind: drawnAs(view),
        categories,
        values: numeric,
        categoryRef,
        valueRef,
        numberFormat: format ?? valueCells[0]!.numFmt,
      })
    }

    const sections = views.length
      ? views.map((view, viewIndex) => ({
          view,
          viewIndex,
          title: item.viewTitles?.[viewIndex] ?? item.title,
          tables: reportTables(view),
        }))
      : [{ view: null, viewIndex: 0, title: item.title, tables: item.tables }]
    for (const section of sections) {
      text(sheet, row++, 2, maxCols, section.title, 12, true)
      if (section.view) {
        const metadata = section.tables.find(
          (table) => table.key === "metadata"
        )?.rows[0]
        const context = [
          reportPeriod({ ...item.row, views: [section.view] }),
          ...["source", "time_zone", "base_from", "base_to"].flatMap((key) =>
            metadata?.[key] ? [`${heading(labels, key)}: ${metadata[key]}`] : []
          ),
        ]
          .filter(Boolean)
          .join(" · ")
        if (context) {
          text(sheet, row, 2, maxCols, context, 10, false, MUTED)
          sheet.getRow(row++).height = 32
        }
      }
      for (const table of section.tables.filter(
        (table) => table.key !== "metadata"
      )) {
        text(
          sheet,
          row++,
          2,
          maxCols,
          labels.sections?.(table.key) ?? labels.data ?? table.key,
          10,
          true,
          MUTED
        )
        const placed = writeTable(sheet, table, row, false)
        row += table.rows.length + 3
        if (!section.view) continue
        const view = section.view
        const cell = (i: number, key: string) => placed.cells[i]?.get(key)
        if (view.kind === "sales" && table.key === "sales")
          table.rows.forEach((source, i) => {
            for (const key of ["gross", "net", "vat", "refunded", "orders"]) {
              const entry = cell(i, key)
              if (entry)
                metricCells.set(
                  `${item.row.id}:${section.viewIndex}:${source.currency}:${source.decimals}:${key}`,
                  entry
                )
            }
          })
        if (!PLOTS.has(drawnAs(view))) continue
        if (
          ["sales", "sales_daily", "breakdown"].includes(view.kind) &&
          ["sales", "ranking"].includes(table.key)
        ) {
          const currencies = [
            ...new Set(
              table.rows.map(
                (source) => `${source.currency}:${source.decimals}`
              )
            ),
          ]
          for (const currency of currencies) {
            const indices = table.rows.flatMap((source, i) =>
              `${source.currency}:${source.decimals}` === currency ? [i] : []
            )
            plot(
              view,
              section.title,
              String(table.rows[indices[0]!]!.currency),
              indices.map((i) =>
                cell(
                  i,
                  view.kind === "breakdown"
                    ? "label"
                    : view.kind === "sales_daily"
                      ? "date"
                      : "currency"
                )!
              ),
              indices.map((i) => cell(i, "gross")!)
            )
          }
        } else if (view.kind === "comparison" && table.key === "comparison") {
          table.rows.forEach((source, i) => {
            const current = cell(i, "current")!,
              base = cell(i, "base")!
            plot(
              view,
              `${section.title} · ${view.base_from.slice(0, 10)} – ${view.base_to.slice(0, 10)}`,
              String(source.currency),
              [
                sheet.getCell(Number(current.row) - i - 1, Number(current.col)),
                sheet.getCell(Number(base.row) - i - 1, Number(base.col)),
              ],
              [current, base]
            )
          })
        } else if (view.kind === "stock" && table.key === "stock") {
          let offset = 0
          for (const group of view.groups) {
            const indices = group.rows.map((_, i) => offset + i)
            const valueCells = indices.map((i) =>
              cell(
                i,
                table.rows[i]!.available === null ? "level" : "available"
              )!
            )
            if (
              valueCells.every(
                (entry) =>
                  typeof entry.value === "string" &&
                  precise(Number(entry.value)) &&
                  entry.value.replace(/[-.]/g, "").replace(/^0+|0+$/g, "")
                    .length <= 15
              )
            )
              plot(
                view,
                section.title,
                group.title ?? "",
                indices.map((i) => cell(i, "label")!),
                valueCells,
                valueCells.map((entry) => Number(entry.value)),
                "General"
              )
            offset += group.rows.length
          }
        } else if (view.kind === "lot_origin" && table.key === "materials") {
          plot(
            view,
            section.title,
            "",
            table.rows.map((_, i) => cell(i, "name")!),
            table.rows.map((_, i) => cell(i, "share")!),
            undefined,
            '0" bp"'
          )
        } else if (view.kind === "attention" && table.key === "counts") {
          for (const key of new Set(
            table.rows.map((source) => source.section)
          )) {
            const indices = table.rows.flatMap((source, i) =>
              source.section === key ? [i] : []
            )
            plot(
              view,
              section.title,
              String(key),
              indices.map((i) => cell(i, "key")!),
              indices.map((i) => cell(i, "count")!)
            )
          }
        }
      }
      row++
    }
    sheet.pageSetup.printArea = `A1:${sheet.getColumn(maxCols).letter}${row}`
    text(
      raw,
      rawRow++,
      2,
      10,
      `${index + 1}. ${item.title} · ${reportPeriod(item.row)}`,
      12,
      true
    )
    for (const table of item.tables) {
      text(
        raw,
        rawRow++,
        2,
        10,
        labels.sections?.(table.key) ?? table.key,
        10,
        true,
        MUTED
      )
      writeTable(raw, table, rawRow, true)
      rawRow += table.rows.length + 3
    }
  }

  let summaryRow = 9
  for (const group of reportSummary(items, labels.locale)) {
    text(
      summary,
      summaryRow++,
      2,
      15,
      `${group.title} · ${group.currency} · ${group.period}`,
      12,
      true
    )
    for (let i = 0; i < group.metrics.length; i++) {
      const metric = group.metrics[i]!,
        col = 2 + (i % 3) * 5,
        top = summaryRow + Math.floor(i / 3) * 4
      text(
        summary,
        top,
        col,
        col + 3,
        heading(labels, metric.key),
        9,
        true,
        MUTED
      )
      const cell = text(summary, top + 1, col, col + 3, "", 20, true)
      const source = metricCells.get(
        `${group.rowId}:${group.viewIndex}:${group.currency}:${group.decimals}:${metric.key}`
      )
      cell.value = source
        ? {
            formula: `${quoteSheet(source.worksheet.name)}!${source.address}`,
            result: source.value as string | number,
          }
        : metric.value
      cell.numFmt = source?.numFmt ?? "@"
      cell.border = { bottom: { style: "thin", color: { argb: GOLD } } }
      summary.getRow(top + 1).height = 36
    }
    summaryRow += Math.ceil(group.metrics.length / 3) * 4 + 1
  }
  const nativeCharts: WorkbookChart[] = []
  for (let i = 0; i < charts.length; i += 2) {
    const pair = charts.slice(i, i + 2)
    const height = Math.max(
      ...pair.map((chart) =>
        chart.kind === "hbar"
          ? Math.min(540, Math.max(300, chart.categories.length * 25))
          : 300
      )
    )
    for (const [index, chart] of pair.entries())
      nativeCharts.push({
        ...chart,
        col: index === 0 ? 1 : 8,
        row: summaryRow - 1,
        width: 480,
        height,
      })
    summaryRow += Math.ceil(height / 28) + 2
  }
  text(
    summary,
    summaryRow++,
    2,
    15,
    labels.selectedResults ?? labels.widget,
    12,
    true
  )
  for (const item of items) {
    text(summary, summaryRow, 2, 7, item.title, 10, true)
    text(summary, summaryRow, 8, 11, reportPeriod(item.row), 10, false, MUTED)
    text(summary, summaryRow, 12, 15, item.row.createdAt ?? "", 9, false, MUTED)
    summary.getRow(summaryRow++).height = 38
  }
  summary.pageSetup.printTitlesRow = "1:7"
  summary.pageSetup.printArea = `A1:O${summaryRow}`
  raw.pageSetup.printArea = `A1:${raw.getColumn(raw.columnCount).letter}${rawRow}`
  raw.pageSetup.fitToWidth = 0
  const bytes = new Uint8Array(await workbook.xlsx.writeBuffer())
  return addWorkbookCharts(bytes, nativeCharts)
}
