export const MAX_ATTACHMENT_BYTES = 10 * 1024 * 1024
export const MAX_ATTACHMENTS = 10
export const ATTACHMENT_ACCEPT =
  "image/jpeg,image/png,image/webp,application/pdf"

/** HTTP status supplies safe guidance without exposing backend or provider diagnostics. */
export function requestFailureKey(status: number) {
  const errors: Record<number, string> = {
    400: "invalid",
    401: "expired",
    403: "forbidden",
    404: "missing",
    409: "conflict",
    410: "conflict",
    413: "size",
    415: "format",
    422: "invalid",
    429: "busy",
    501: "unavailable",
  }
  return `requestErrors.${errors[status] ?? "unverified"}`
}
export type TranscriptRow = {
  id: string
  role: "user" | "agent" | "action"
  text: string
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
export function mergeTranscript(rows: TranscriptRow[], row: TranscriptRow) {
  const index = rows.findIndex((item) => item.id === row.id)
  if (index < 0) return [...rows.slice(-199), row]
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
