import { failureReason } from "@/lib/voice/client-contract"
export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024
export const MAX_ATTACHMENTS = 10
export const ATTACHMENT_ACCEPT =
  "image/jpeg,image/png,image/webp,application/pdf"

/** HTTP status supplies safe guidance without exposing backend or provider diagnostics. */
export function requestFailureKey(status: number) {
  return `requestErrors.${failureReason(status)}`
}
export type TranscriptRow = {
  id: string
  role: "user" | "agent" | "action" | "link"
  text: string
  /** Office page a "link" row opens. */
  href?: string
}

const SAFE_HREF =
  /^(\/(products|orders|invoices|lots|stock|supply|till|settings)(\/|\?|$)|https:\/\/)/

/** Splits "[label](url)" out of agent text; only office pages and https become links. */
export function splitLinks(text: string): { text: string; href?: string }[] {
  const parts: { text: string; href?: string }[] = []
  let last = 0
  for (const match of text.matchAll(
    /\[([^\]\n]{1,120})\]\(([^)\s]{1,500})\)/g
  )) {
    const [whole, label, href] = match
    if (!SAFE_HREF.test(href!)) continue
    if (match.index > last) parts.push({ text: text.slice(last, match.index) })
    parts.push({ text: label!, href })
    last = match.index + whole.length
  }
  if (last < text.length) parts.push({ text: text.slice(last) })
  return parts
}

/** What the agent hears about an outcome: record names, never links. */
export function outcomeForAgent(outcome: {
  links?: { label: string | null; href: string }[]
  [field: string]: unknown
}) {
  const { links, ...rest } = outcome
  const records = links?.flatMap((link) => (link.label ? [link.label] : []))
  return records?.length ? { ...rest, records } : rest
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
  file: { type: string; size: number },
  count: number
) {
  if (count >= MAX_ATTACHMENTS) return "count"
  if (!ATTACHMENT_ACCEPT.split(",").includes(file.type)) return "type"
  if (!file.size) return "empty"
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
