import type { TranscriptRow } from "./helpers"
import { formatMoney } from "@genesy/ui/lib/money"
import { drawnAs, type ResultView } from "./views"

export type ReportCell = string | number | boolean | null
export type ReportTable = {
  key: string
  columns: string[]
  rows: Record<string, ReportCell>[]
}
export type ReportRow = TranscriptRow & {
  requestId?: string
  createdAt?: string
}
export type ReportItem = {
  row: ReportRow
  title: string
  viewTitles?: string[]
  tables: ReportTable[]
}
export type ReportLabels = {
  locale?: string
  minorUnits?: string
  title: string
  snapshot: string
  captured: string
  widget: string
  period: string
  columns: (key: string) => string
  brand?: string
  businessName?: string
  summary?: string
  details?: string
  source?: string
  currency?: string
  continued?: string
  rawData?: string
  data?: string
  selectedResults?: string
  chartTitle?: string
  page?: (current: number, total: number) => string
  sections?: (tableKey: string) => string
}

export type ReportSummary = {
  rowId: string
  viewIndex: number
  title: string
  period: string
  captured?: string
  currency: string
  decimals: number
  metrics: { key: string; value: string; raw: number }[]
}

/** Direct totals only: rankings and daily snapshots cannot establish complete sales totals. */
export function reportSummary(
  items: ReportItem[],
  locale = "en"
): ReportSummary[] {
  return items.flatMap((item) =>
    (item.row.views ?? []).flatMap((view, viewIndex) =>
      view.kind !== "sales"
        ? []
        : view.rows.map((row) => ({
            rowId: item.row.id,
            viewIndex,
            title: item.viewTitles?.[viewIndex] ?? item.title,
            period: reportPeriod({ ...item.row, views: [view] }),
            captured: item.row.createdAt,
            currency: row.currency,
            decimals: row.decimals,
            metrics: (
              ["gross", "net", "vat", "orders", "refunded"] as const
            ).flatMap((key) => {
              const raw = row[key]
              return raw === null
                ? []
                : [
                    {
                      key,
                      raw,
                      value:
                        key === "orders"
                          ? new Intl.NumberFormat(locale).format(raw)
                          : formatMoney(
                              raw,
                              row.decimals,
                              row.currency,
                              locale
                            ),
                    },
                  ]
            }),
          }))
    )
  )
}
const PLOTS = new Set(["line", "sparkline", "bar", "hbar", "pie", "donut"])

export function fitReportChart(
  width: number,
  height: number,
  maxWidth: number,
  maxHeight: number
) {
  const scale = Math.min(maxWidth / width, maxHeight / height)
  return { width: width * scale, height: height * scale }
}

/** PDF uses business amounts; workbook tables retain the exact original units and helper fields. */
export function pdfReportTables(
  tables: ReportTable[],
  locale: string
): ReportTable[] {
  return tables
    .filter((table) => table.key !== "metadata")
    .map((table) => ({
      ...table,
      columns: table.columns.filter(
        (column) => column !== "decimals" && !column.endsWith(".decimals")
      ),
      rows: table.rows.map((row) =>
        Object.fromEntries(
          Object.entries(row).map(([column, value]) => {
            if (
              typeof value !== "number" ||
              !(
                column.endsWith(".minor") ||
                ["gross", "net", "vat", "refunded", "current", "base"].includes(
                  column
                )
              )
            )
              return [column, value]
            const prefix = column.endsWith(".minor")
              ? column.slice(0, -"minor".length)
              : ""
            const currency = row[`${prefix}currency`]
            const decimals = row[`${prefix}decimals`]
            return [
              column,
              typeof currency === "string" && typeof decimals === "number"
                ? formatMoney(value, decimals, currency, locale)
                : value,
            ]
          })
        )
      ),
    }))
}

export const reportRows = (rows: ReportRow[]) =>
  rows.filter((row) => row.role === "result" && row.views?.length)
export const latestChart = (rows: ReportRow[]) =>
  reportRows(rows)
    .reverse()
    .find((row) => row.views?.some((v) => PLOTS.has(drawnAs(v))))
export const answerResultIds = (rows: ReportRow[], requestId: string | null) =>
  requestId
    ? reportRows(rows)
        .filter((row) => row.requestId === requestId)
        .map((row) => row.id)
    : []
export const retainedSelection = (selected: string[], rows: ReportRow[]) => {
  const retained = new Set(reportRows(rows).map((row) => row.id))
  return selected.filter((id) => retained.has(id))
}

/** Flatten scalar fields only: decimal text stays text, nested money keeps its exact units. */
function cells(value: object, prefix = ""): Record<string, ReportCell> {
  return Object.fromEntries(
    Object.entries(value).flatMap(([key, item]) => {
      const name = `${prefix}${key}`
      if (Array.isArray(item)) return []
      if (item !== null && typeof item === "object")
        return Object.entries(cells(item as object, `${name}.`))
      return [[name, item === undefined ? null : (item as ReportCell)]]
    })
  )
}
function table(key: string, rows: object[]): ReportTable {
  const flat = rows.map((row) => cells(row))
  return { key, columns: [...new Set(flat.flatMap(Object.keys))], rows: flat }
}

/** Every field retained by ResultView, without parsing formatted chart labels or deriving totals. */
export function reportTables(view: ResultView): ReportTable[] {
  const metadata = table("metadata", [view])
  let data: ReportTable[]
  switch (view.kind) {
    case "sales":
      data = [table("sales", view.rows)]
      break
    case "sales_daily":
      data = [
        table(
          "sales",
          view.currencies.flatMap((c) =>
            c.days.map((day) => ({
              currency: c.currency,
              decimals: c.decimals,
              ...day,
            }))
          )
        ),
      ]
      break
    case "records":
      data = [table("records", view.rows)]
      break
    case "lot_origin":
      data = [table("materials", view.materials)]
      break
    case "trail":
      data = [table("events", view.events)]
      break
    case "attention":
      data = [
        table(
          "counts",
          view.sections.flatMap((s) =>
            Object.entries(s.counts).map(([key, count]) => ({
              section: s.key,
              key,
              count,
            }))
          )
        ),
        table(
          "items",
          view.sections.flatMap((s) =>
            s.items.map((item) => ({ section: s.key, ...item }))
          )
        ),
      ]
      break
    case "breakdown":
      data = [
        table(
          "ranking",
          view.currencies.flatMap((c) =>
            c.rows.map((row) => ({
              currency: c.currency,
              decimals: c.decimals,
              ...row,
            }))
          )
        ),
      ]
      break
    case "comparison":
      data = [table("comparison", view.currencies)]
      break
    case "stock":
      data = [
        table(
          "stock",
          view.groups.flatMap((g) =>
            g.rows.map((row) => ({ warehouse: g.title, ...row }))
          )
        ),
      ]
      break
    case "search":
      data = [
        table(
          "records",
          view.groups.flatMap((g) =>
            g.rows.map((row) => ({ resource: g.key, ...row }))
          )
        ),
      ]
      break
    case "document":
      data = [
        table("facts", view.facts),
        table("lines", view.lines),
        table("totals", view.totals),
      ]
      break
    case "till_sales":
      data = [table("sales", view.rows)]
      break
    case "till_shifts":
      data = [table("shifts", view.rows)]
      break
  }
  return [metadata, ...data].filter((t) => t.rows.length)
}

export function reportPeriod(row: ReportRow) {
  const view = row.views?.[0]
  return view && "from" in view
    ? `${view.from.slice(0, 10)} – ${view.to.slice(0, 10)}`
    : ""
}

/** Selection freezes before asynchronous chart/font loading; a new answer cannot change export. */
export function reportItems(
  rows: ReportRow[],
  selected: string[],
  title: (view: ResultView) => string
): ReportItem[] {
  const ids = new Set(selected)
  return reportRows(rows)
    .filter((row) => ids.has(row.id))
    .map((row) => {
      const frozen = structuredClone(row)
      return {
        row: frozen,
        title: title(frozen.views![0]!),
        viewTitles: frozen.views!.map(title),
        tables: frozen.views!.flatMap(reportTables),
      }
    })
}
