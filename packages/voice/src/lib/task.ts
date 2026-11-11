import type { VoiceOutcome, VoiceReview } from "./client-contract"
import type { BatchProgress } from "./continuity"
import { taskPresentation, type TranscriptRow, type VoiceProgress } from "./helpers"
import type { RecordRow } from "./views"

/**
 * The active task as people see it: what Genesy does now, from the provider's real state only.
 * Milestones are fixed honest words and real step counts, never percentages.
 */
export type TaskState =
  | "preparing"
  | "running"
  | "waiting_input"
  | "needs_approval"
  | "paused"
  | "failed"
  | "partial"
  | "completed"
  | "cancelled"
  | "unknown"
export type BatchCounts = {
  total: number
  done: number
  failed: number
  unknown: number
  notRun: number
}
export type TaskView = { state: TaskState; milestone: string; counts?: BatchCounts }
export type TaskInput = {
  review: VoiceReview | null
  busy: boolean
  thinking: boolean
  progress: VoiceProgress | null
  batchProgress: BatchProgress | null
  outcome: VoiceOutcome | null
  /** The review the person decided (approved or cancelled); its outcome is this task's end. */
  decided: VoiceReview | null
  /** A choice is open for the person (see clarifyOf). */
  clarifying: boolean
}

const OUTCOME: Record<string, TaskState> = {
  completed: "completed",
  partial: "partial",
  failed: "failed",
  expired: "failed",
  cancelled: "cancelled",
  not_run: "cancelled",
  paused: "paused",
}
const MILESTONE: Partial<Record<TaskState, string>> = { unknown: "checking" }

export function batchCounts(batch: BatchProgress): BatchCounts {
  const counts = { total: batch.steps.length, done: 0, failed: 0, unknown: 0, notRun: 0 }
  for (const { status } of batch.steps)
    if (status === "completed") counts.done++
    else if (status === "failed" || status === "partial") counts.failed++
    else if (status === "unknown" || status === "queued") counts.unknown++
    else counts.notRun++
  return counts
}

export function taskView(input: TaskInput): TaskView | null {
  const batch = input.batchProgress
  const counts = batch && batch.steps.length ? { counts: batchCounts(batch) } : {}
  if (batch?.state === "paused") return { state: "paused", milestone: "paused", ...counts }
  if (input.review)
    return input.busy
      ? { state: "running", milestone: "executing", ...counts }
      : { state: "needs_approval", milestone: "waitingConfirmation" }
  if (input.clarifying) return { state: "waiting_input", milestone: "waitingInput" }
  if (input.decided && input.outcome) {
    const state = OUTCOME[taskPresentation(input.outcome.status)] ?? "unknown"
    return { state, milestone: MILESTONE[state] ?? state, ...counts }
  }
  if (input.progress?.status === "preparing_review")
    return { state: "preparing", milestone: "preparingReview" }
  // A question being answered is not a task: no capsule for plain reads.
  return null
}

type Obj = Record<string, unknown>
const obj = (v: unknown): v is Obj => !!v && typeof v === "object" && !Array.isArray(v)
const text = (v: unknown) => (typeof v === "string" ? v : typeof v === "number" ? String(v) : null)
/** "150.00" → "150", "1.50" → "1.5": the same scale the review's snapshot shows. */
const trim = (v: string) => (v.includes(".") ? v.replace(/\.?0+$/, "") : v)

export type ReceiptRow = {
  label: string
  before: string | null
  /** Confirmed by the API: its own answer ("api") or a read after the write ("read"); null until known. */
  after: string | null
  source: "api" | "read"
  /** The warehouse/bin a read-back row reads (stockAt). */
  slot?: { warehouse: string; bin: string | null }
}
export type Receipt = {
  status: string
  operationId: string | null
  record: string | null
  /** Assistant action id: the audit reference. */
  reference: string | null
  /** Read this product's stock again to confirm the new value. */
  readBack: { product: string } | null
  rows: ReceiptRow[]
}
/** Read-back values by `${warehouse_id}:${bin_id ?? ""}`. */
export type ReadBack = Record<string, string>
export const slotKey = (warehouse: string, bin: string | null | undefined) =>
  `${warehouse}:${bin ?? ""}`

/**
 * What an approved action did, from the server only: the review's snapshot is "before", the API's
 * answer or a read after the write is "after". Nothing is taken from the proposal as the result.
 */
export function receiptOf(
  review: VoiceReview,
  outcome: VoiceOutcome,
  readBack: ReadBack | undefined
): Receipt {
  const status = ["completed", "partial", "failed", "cancelled"].includes(outcome.status)
    ? outcome.status
    : "unknown"
  const top = (outcome as { actionId?: unknown }).actionId
  const data = obj(outcome.data) ? outcome.data : {}
  const details = review.details
  const record = obj(details.record) ? text(details.record.label) : null
  const base = {
    status,
    operationId: review.operationId ?? null,
    record,
    reference: text(top) ?? text(data.actionId),
    readBack: null,
    rows: [] as ReceiptRow[],
  }
  const confirmed = status === "completed"
  if (review.operationId === "stock_mutate") {
    const before = text(data.before)
    const after = text(data.after)
    const warehouse = obj(data.warehouse) ? text(data.warehouse.name) : null
    return confirmed && before !== null && after !== null
      ? { ...base, rows: [{ label: warehouse ?? "", before: trim(before), after: trim(after), source: "api" }] }
      : base
  }
  if (review.operationId === "stock_set_levels") {
    const target = obj(details.target) ? text(details.target.product) : null
    const current = obj(details.current) && obj(details.current.locked) ? details.current.locked : {}
    const slots = Array.isArray(current.slots) ? current.slots.filter(obj) : []
    const rows = slots.map((slot) => {
      const at = { warehouse: String(slot.warehouse_id), bin: text(slot.bin_id) }
      return {
        label: text(slot.warehouse_name) ?? "",
        before: text(slot.before),
        after: confirmed && readBack ? (readBack[slotKey(at.warehouse, at.bin)] ?? null) : null,
        source: "read" as const,
        slot: at,
      }
    })
    return { ...base, readBack: confirmed && target ? { product: target } : null, rows }
  }
  return base
}

/** The stock a review slot names: that warehouse and bin, no lot; no row is zero (as the snapshot). */
export function stockAt(
  rows: { warehouse: unknown; bin: unknown; lot: unknown; stock_lvl: string }[],
  warehouse: string,
  bin: string | null
) {
  const id = (v: unknown) => (obj(v) ? text(v.id) : null)
  const row = rows.find(
    (r) => id(r.warehouse) === warehouse && id(r.bin) === bin && !r.lot
  )
  return row ? trim(row.stock_lvl) : "0"
}

/**
 * A choice to offer: the newest answer lists 2–5 products and Genesy then asked a question the
 * person has not answered yet. Picking one only sends words; the agent still proposes the change.
 */
export function clarifyOf(messages: TranscriptRow[]): { resultId: string; rows: RecordRow[] } | null {
  let question = false
  for (let i = messages.length - 1; i >= 0; i--) {
    const row = messages[i]!
    if (row.role === "user") return null
    if (row.role === "agent" && row.text.trim().endsWith("?")) question = true
    if (row.role !== "result") continue
    const view = row.views?.find((v) => v.kind === "records" && v.resource === "products")
    if (!question || !view || view.kind !== "records" || view.rows.length < 2) return null
    return { resultId: row.id, rows: view.rows.slice(0, 5) }
  }
  return null
}
