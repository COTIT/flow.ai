import { failureReason } from "./client-contract"
import { drawnAs, type Kind, type ResultView } from "./views"
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024
export const MAX_ATTACHMENTS = 10
const XLSX = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
const MAX_SHEET_BYTES = 2 * 1024 * 1024
const PHOTO_TYPES = ["image/jpeg", "image/png", "image/webp", "application/pdf"]
export const ATTACHMENT_ACCEPT = [...PHOTO_TYPES, XLSX, ".csv", ".xlsx"].join(
  ","
)

/** Price-list sheet type by extension: browsers label .csv as "", text/csv or ms-excel. */
export function sheetMime(file: { type: string; name?: string }) {
  if (/\.csv$/i.test(file.name ?? "") || file.type === "text/csv")
    return "text/csv"
  if (/\.xlsx$/i.test(file.name ?? "") || file.type === XLSX) return XLSX
  return null
}

/** HTTP status supplies safe guidance without exposing backend or provider diagnostics. */
export function requestFailureKey(status: number) {
  return `requestErrors.${failureReason(status)}`
}
export type TranscriptRow = {
  id: string
  role: "user" | "agent" | "action" | "link" | "result"
  text: string
  historical?: boolean
  delivery?: "queued" | "sent_unconfirmed"
  /** Charts or cards a "result" row shows. */
  views?: ResultView[]
  /** Office page a "link" row opens. */
  href?: string
  /** The read a "result" row came from, so the agent can redo or refine it. */
  operation?: string
  /** Canonical user request that produced this result, shared across its datasets. */
  requestId?: string
  /** When the displayed result snapshot was received. */
  createdAt?: string
}

/** Office pages and https: the agent-text links Office opens. */
export const SAFE_HREF =
  /^(\/(products|orders|invoices|lots|stock|supply|till|settings)(\/|\?|$)|https:\/\/)/

/** Splits "[label](url)" out of agent text; only hrefs `safe` allows become links. */
export function splitLinks(
  text: string,
  safe: RegExp = SAFE_HREF
): { text: string; href?: string }[] {
  const parts: { text: string; href?: string }[] = []
  let last = 0
  for (const match of text.matchAll(
    /\[([^\]\n]{1,120})\]\(([^)\s]{1,500})\)/g
  )) {
    const [whole, label, href] = match
    if (!safe.test(href!)) continue
    if (match.index > last) parts.push({ text: text.slice(last, match.index) })
    parts.push({ text: label!, href })
    last = match.index + whole.length
  }
  if (last < text.length) parts.push({ text: text.slice(last) })
  return parts
}

/** A field key without a label, in sentence case like the labelled ones: "purchase_price" → "Purchase price". */
export function fieldName(key: string) {
  const words = key.replaceAll("_", " ").replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase()
  return words.charAt(0).toUpperCase() + words.slice(1)
}

/** The result kinds the screen draws (VoiceResult), as the agent should think of them. */
const PICTURES: Record<string, "chart" | "list"> = {
  sales: "chart",
  sales_daily: "chart",
  records: "list",
  attention: "list",
  lot_origin: "list",
  trail: "list",
  comparison: "chart",
  breakdown: "chart",
  stock: "list",
  till_sales: "list",
  till_shifts: "list",
  search: "list",
  document: "list",
}

const count = (v: unknown) => (Array.isArray(v) ? v.length : 0)
const countIn = (list: unknown, key: string) =>
  Array.isArray(list) ? list.reduce((n: number, x) => n + count((x as Record<string, unknown> | null)?.[key]), 0) : 0
/** Data points a picture draws: rows (also per group), lines, days per currency, items per section, events, materials. */
const pointsOf = (view: Record<string, unknown>) =>
  count(view.rows) + count(view.materials) + count(view.events) +
  countIn(view.currencies, "days") + countIn(view.currencies, "rows") + countIn(view.sections, "items") +
  countIn(view.groups, "rows") + count(view.lines)

/** Office kept the default drawing: the agent says why in one sentence and offers the closest fit. */
const refusal = (note: unknown) => {
  if (!note || typeof note !== "object") return ""
  const { asked, reason, closest } = note as { asked?: string; reason?: string; closest?: string }
  return ` The person asked for ${asked === "unknown" ? "a drawing this screen does not have" : `a ${asked}`}, which does not fit: ${reason}. Say that in one sentence and offer a ${closest} instead.`
}

/** What the agent hears about an outcome: record names, never links; that a picture is shown. */
export function outcomeForAgent(outcome: {
  links?: { label: string | null; href: string }[]
  [field: string]: unknown
}): Record<string, unknown> & {
  on_screen?: { kind: "chart" | "list"; shown_as: Kind; points: number; instruction: string }
} {
  // The picture is for the screen only; the agent keeps `data` so what it says stays grounded.
  const { links, view, ...rest } = outcome
  const records = links?.flatMap((link) => (link.label ? [link.label] : []))
  const kind = PICTURES[(view as { kind?: string } | undefined)?.kind ?? ""]
  return {
    ...rest,
    ...(records?.length ? { records } : {}),
    ...(kind
      ? {
          on_screen: {
            kind,
            shown_as: drawnAs(view as ResultView),
            points: pointsOf(view as Record<string, unknown>),
            instruction: `This result is on the person's screen as a ${kind}. Answer in one or two sentences: the total or the direction and at most one notable point; never read the ${kind === "chart" ? "points" : "items"} one by one.${refusal(rest.presentation_note)}`,
          },
        }
      : {}),
  }
}
export function referenceSize(bytes: number, locale: string) {
  const unit = bytes >= 1024 * 1024 ? "MiB" : bytes >= 1024 ? "KiB" : "B"
  const divisor = unit === "MiB" ? 1024 * 1024 : unit === "KiB" ? 1024 : 1
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(bytes / divisor)} ${unit}`
}
export function taskPresentation(status: string, batchState?: string) {
  if (status === "not_started") return "not_run"
  if (
    status === "awaiting_review" &&
    batchState &&
    !["proposed", "approved", "running"].includes(batchState)
  )
    return "not_run"
  return [
    "awaiting_review",
    "running",
    "completed",
    "cancelled",
    "expired",
    "failed",
    "queued",
    "partial",
    "paused",
  ].includes(status)
    ? status
    : "unknown"
}
export function attachmentProblem(
  file: { type: string; size: number; name?: string },
  count: number
) {
  if (count >= MAX_ATTACHMENTS) return "count"
  const sheet = sheetMime(file)
  if (!sheet && !PHOTO_TYPES.includes(file.type)) return "type"
  if (!file.size) return "empty"
  if (sheet && file.size > MAX_SHEET_BYTES) return "sheetSize"
  if (file.size > MAX_ATTACHMENT_BYTES) return "size"
  return null
}
const sameLine = (a: string, b: string) =>
  a.trim().replace(/\s+/g, " ").toLowerCase() ===
  b.trim().replace(/\s+/g, " ").toLowerCase()

export function mergeTranscript(rows: TranscriptRow[], row: TranscriptRow) {
  const index = rows.findIndex((item) => item.id === row.id)
  if (index < 0) {
    // The agent speaks its "checking" line before each tool call; one per user turn is enough.
    if (row.role === "agent")
      for (const item of [...rows].reverse()) {
        if (item.role === "user") break
        if (item.role === "agent" && sameLine(item.text, row.text)) return rows
      }
    return [...rows.slice(-199), row]
  }
  return rows.map((item, i) => (i === index ? row : item))
}

/** Provider metadata must match the server allocation before any capabilities open. */
export function matchesIssuedConversation(
  issued: string | undefined,
  connected: string
) {
  return Boolean(issued) && issued === connected
}

export type VoiceProgress = {
  status: string
  steps: string[]
  narration: string
}
export function progressOf(
  status: string,
  result?: { status: string; steps?: string[]; narration?: string }
): VoiceProgress {
  return {
    status: result?.status ?? status,
    steps: result?.steps ?? [],
    narration: result?.narration ?? "",
  }
}
/** Truthful "still working" stage: fires only while the request is still in flight. */
export async function whenSlow<T>(
  work: Promise<T>,
  onSlow: () => void,
  ms = 800
): Promise<T> {
  const timer = setTimeout(onSlow, ms)
  try {
    return await work
  } finally {
    clearTimeout(timer)
  }
}

/** Expressive TTS tags ("[happy]") shape the voice only; the transcript shows the words. */
export function spokenText(text: string) {
  return text
    .replace(/\s*\[[a-z][a-z _-]*\]\s*/g, " ")
    .replace(/\s{2,}/g, " ")
    .trim()
}

/** A major-unit decimal string from the API ("11.6600") with its currency: Intl formats the
 * string as-is (no float); `decimals` only sets how many places show, rounding for display. */
export function formatPrice(price: string, decimals: number, currency: string, locale: string) {
  return new Intl.NumberFormat(locale, {
    style: "currency",
    currency,
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
    // TS's lib types lack the string overload; runtimes since Node 19 / 2023 browsers have it.
  }).format(price as unknown as number)
}

/** An API quantity ("20.00", "1.50") as a count: only trailing zeros go ("20", "1.5"). The string
 * goes into Intl as-is, so the value is never rounded; the locale adds its separators. */
export const formatQuantity = (qty: string, locale: string) =>
  new Intl.NumberFormat(locale, { maximumFractionDigits: 20 }).format(qty as unknown as number)
