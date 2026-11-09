import "server-only"
import type { PreparedAction } from "./catalog"
import { callBackend, type Outcome, type VoiceContext } from "./server"
import { actionStatus, actionTransport, type StoredAction } from "./actions"
import { assistantRequest } from "./bridge"
import { getOperation, isToolAllowed, ToolUnavailableError } from "./catalog"
import type { MultipartReview } from "./multipart"
import { VoiceError, voiceFailure } from "./server"

export type StoredBatch = {
  id: string
  session_id: string
  fingerprint: string
  state: string
  next_index: number
  action_count: number
  expires_at: string
  reason?: string | null
  recoverable?: boolean
  steps: StoredAction[]
}
export type BatchExecution = {
  batch: StoredBatch
  actions: PreparedAction[]
  context: VoiceContext
  files: Map<string, Blob>
  signal: AbortSignal
  refresh: () => Promise<VoiceContext>
  transports?: (MultipartReview | undefined)[]
}

const batchCopy = {
  en: {
    summary: (count: number) => `Fixed batch: ${count} actions`,
    target: "fixed batch",
    completed: "Every reviewed batch step completed.",
    stopped:
      "The fixed batch stopped. Remaining actions were not started; check each recorded step before preparing another review.",
    review: "Review the exact fixed batch in Office.",
    running: "The fixed batch is running. No remaining step is complete yet.",
    next: (count: number) =>
      `Done. ${count} more changes are left; they need their own review. Shall I prepare the next part?`,
  },
  nl: {
    summary: (count: number) => `Vaste reeks: ${count} acties`,
    target: "vaste reeks",
    completed: "Elke beoordeelde actie in de reeks is voltooid.",
    stopped:
      "De vaste reeks is gestopt. Resterende acties zijn niet gestart; controleer elke vastgelegde stap voordat u een nieuwe beoordeling voorbereidt.",
    review: "Bekijk de exacte vaste reeks in Office.",
    running:
      "De vaste reeks wordt uitgevoerd. Resterende stappen zijn nog niet voltooid.",
    next: (count: number) =>
      `Klaar. Er zijn nog ${count} wijzigingen over; die krijgen een eigen beoordeling. Zal ik het volgende deel voorbereiden?`,
  },
  sw: {
    summary: (count: number) => `Kundi maalumu: hatua ${count}`,
    target: "kundi maalumu",
    completed: "Kila hatua iliyokaguliwa katika kundi imekamilika.",
    stopped:
      "Kundi maalumu limesimama. Hatua zilizobaki hazijaanza; kagua kila hatua iliyorekodiwa kabla ya kuandaa ukaguzi mwingine.",
    review: "Kagua kundi maalumu halisi katika Office.",
    running: "Kundi maalumu linaendelea. Hatua zilizobaki bado hazijakamilika.",
    next: (count: number) =>
      `Imekamilika. Mabadiliko ${count} zaidi yamebaki; yanahitaji ukaguzi wake. Nikuandalie sehemu inayofuata?`,
  },
}
export function batchWords(language = "en") {
  return batchCopy[language === "nl" || language === "sw" ? language : "en"]
}

/** Preserve HTTP meaning while user-facing failures follow the owned session language. */
export function batchFailure(error: unknown, language = "en") {
  if (language !== "nl" && language !== "sw") return voiceFailure(error)
  const status = error instanceof VoiceError || error instanceof ToolUnavailableError ? error.status : 503
  const messages =
    language === "nl"
      ? {
          permission:
            "Deze reeks is niet beschikbaar met uw huidige aanmelding en rechten.",
          stale:
            "Deze reeks is gewijzigd, gestopt of verlopen. Controleer de vastgelegde stappen en beoordeel een nieuwe reeks.",
          invalid:
            "Controleer de exacte acties, bevestigingszin en oorspronkelijke bijlagen van deze reeks.",
          unavailable:
            "De reeks kon niet worden verwerkt. Controleer vastgelegde stappen voordat u opnieuw begint.",
        }
      : {
          permission:
            "Kundi hili halipatikani kwa kuingia na ruhusa zako za sasa.",
          stale:
            "Kundi hili limebadilika, limesimama au limeisha muda. Kagua hatua zilizorekodiwa na ukague kundi jipya.",
          invalid:
            "Kagua hatua halisi, sentensi ya uthibitisho na viambatisho vya asili vya kundi hili.",
          unavailable:
            "Kundi halikuweza kuchakatwa. Kagua hatua zilizorekodiwa kabla ya kuanza tena.",
        }
  const message =
    status === 401 || status === 403
      ? messages.permission
      : status === 404 || status === 409
        ? messages.stale
        : status === 400 || status === 422
          ? messages.invalid
          : messages.unavailable
  return voiceFailure(new VoiceError(message, status))
}

/** Public status includes only verified ledger receipts, never controller credentials/results. */
function terminalBatchStatus(batch: StoredBatch, language = "en"): Outcome {
  const copy = batchWords(language)
  const steps = (batch.steps ?? []).map((step) => {
    const outcome = actionStatus(step)
    return {
      ...outcome,
      status:
        outcome.status === "awaiting_review" ? "not_started" : outcome.status,
    }
  })
  const states = steps.map((step) => step.status)
  const status =
    batch.state === "completed"
      ? "completed"
      : states.includes("unknown")
        ? "unknown"
        : states.includes("queued")
          ? "queued"
          : batch.state === "cancelled" &&
              !states.some(
                (state) =>
                  state === "completed" ||
                  state === "partial" ||
                  state === "running"
              )
            ? "cancelled"
            : "partial"
  return {
    status,
    message: batch.state === "completed" ? copy.completed : copy.stopped,
    data: {
      kind: "batch",
      batchId: batch.id,
      state: batch.state,
      nextIndex: batch.next_index,
      actionCount: batch.action_count,
      reason: batch.reason,
      summary: copy.summary(batch.action_count),
      recoverable: Boolean(batch.recoverable),
      steps,
    },
  }
}

/** Pending batches expose progress without implying verified completion. */
export function batchStatus(batch: StoredBatch, language = "en") {
  const result = terminalBatchStatus(batch, language)
  const copy = batchWords(language)
  if (batch.state === "proposed" || batch.state === "approved")
    return {
      ...result,
      status: "awaiting_review",
      message: copy.review,
    }
  if (batch.state === "running")
    return {
      ...result,
      status: "running",
      message: copy.running,
    }
  return result
}

/** Request-bound controller; Rust remains the only admission and ordering authority. */
export async function executeBatchController(
  run: BatchExecution,
  backend = callBackend
): Promise<Outcome> {
  const base = `/batches/${encodeURIComponent(run.batch.id)}`
  let current = run.batch
  const stop = async (reason: string) => {
    try {
      current = await assistantRequest<StoredBatch>(
        run.context,
        `${base}/pause`,
        "POST",
        { session_id: run.context.sessionId, reason }
      )
    } catch {
      current = { ...current, state: "paused", reason, recoverable: false }
    }
  }
  if (run.signal.aborted) {
    await stop("disconnected")
    return terminalBatchStatus(current, run.context.language)
  }
  const started = await assistantRequest<{
    batch: StoredBatch
    execution_token: string
  }>(
    run.context,
    `${base}/start`,
    "POST",
    { session_id: run.context.sessionId },
    true
  )
  if (!/^[0-9a-f]{64}$/.test(started.execution_token))
    throw new VoiceError("Batch controller proof was not returned.", 503)
  current = started.batch
  for (let index = 0; index < run.actions.length; index++) {
    if (run.signal.aborted) {
      await stop("disconnected")
      break
    }
    if (Date.parse(run.batch.expires_at) <= Date.now()) {
      await stop("expired")
      break
    }
    let context: VoiceContext
    try {
      context = await run.refresh()
      if (
        context.token !== run.context.token ||
        context.workspaceId !== run.context.workspaceId
      )
        throw new VoiceError("The batch login changed.", 403)
      context = {
        ...context,
        sessionId: run.context.sessionId,
        language: run.context.language,
      }
      current = await assistantRequest<StoredBatch>(context, base)
      if (
        current.id !== run.batch.id ||
        current.fingerprint !== run.batch.fingerprint ||
        current.state !== "running" ||
        current.next_index !== index
      )
        break
      const action = run.actions[index]!
      if (
        current.steps[index]?.id !== run.batch.steps[index]?.id ||
        current.steps[index]?.operation_id !== action.operationId
      )
        throw new VoiceError("The reviewed batch changed.", 409)
      const operation = getOperation(action.operationId)
      if (
        !operation ||
        operation.disposition !== "execute" ||
        !isToolAllowed(operation, context.permissions)
      )
        throw new VoiceError("Batch permission changed.", 403)
      const handles = new Set(Object.values(action.attachments ?? {}).flat())
      const files = new Map(
        [...run.files].filter(([handle]) => handles.has(handle))
      )
      const expected = run.transports?.[index]
      const transport = await actionTransport(
        action,
        context,
        files,
        expected?.boundary
      )
      if (
        (transport || expected) &&
        JSON.stringify(transport?.reviewMetadata) !== JSON.stringify(expected)
      )
        throw new VoiceError("A batch attachment changed.", 409)
      if (run.signal.aborted) {
        await stop("disconnected")
        break
      }
      const outcome = await backend(
        action,
        {
          ...context,
          batch: { id: current.id, token: started.execution_token },
          ...(transport
            ? {
                transport: {
                  bytes: transport.bytes,
                  contentType: transport.contentType,
                },
              }
            : {}),
        },
        current.steps[index]!.id
      )
      if (outcome.status !== "completed") {
        await stop(
          outcome.status === "cancelled" ? "cancelled" : outcome.status
        )
        break
      }
      current = await assistantRequest<StoredBatch>(
        context,
        `${base}/advance`,
        "POST",
        {
          session_id: context.sessionId,
          execution_token: started.execution_token,
          action_id: current.steps[index]!.id,
        },
        true
      )
      if (run.signal.aborted) {
        await stop("disconnected")
        break
      }
      if (current.state !== "running") break
    } catch (error) {
      await stop(
        error instanceof VoiceError && [401, 403].includes(error.status)
          ? "authorization"
          : "unknown"
      )
      break
    }
  }
  return terminalBatchStatus(current, run.context.language)
}
