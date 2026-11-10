import "server-only"
import { createHash } from "node:crypto"
import { assistantRequest } from "./bridge"
import {
  actionTransport,
  businessDraft,
  reviewStep,
  restoreAction,
} from "./actions"
import {
  getOperation,
  isToolAllowed,
  prepareAction,
  type PreparedAction,
} from "./catalog"
import {
  batchStatus,
  batchWords,
  executeBatchController,
  type StoredBatch,
} from "./batch-controller"
import { confirmationPhrase, matchesSpokenConfirmation } from "@genesy/voice/lib/confirmation"
import {
  createReview,
  verifyReview,
  checkpointReview,
  executeAction,
  cancelReview,
} from "./workflow"
import { voiceSetting } from "./settings"
import { VoiceError, type VoiceContext } from "./server"
import type { MultipartReview } from "./multipart"
import { IMPORT_PAGE } from "./sheet"
import type { VoiceOutcome, VoiceReviewStep } from "@genesy/voice/lib/client-contract"

export const BATCH_PROTOCOL_VERSION = 1
export const BATCH_PREPARE_OPERATION = "assistant_batch_prepare"
export const BATCH_STATUS_OPERATION = "assistant_batch_status"
/** Model-built batches: rows a person can still read inside the 120 s review window. */
export const MODEL_BATCH_MAX = 25
/** Changes the agent says are left after this part; each part gets its own review. */
const REMAINING_MAX = 10000
type Plan = {
  version: 1
  fingerprint: string
  actionIds: string[]
  actions: PreparedAction[]
  transports: (MultipartReview | null)[]
  remaining?: number
}
const isRemaining = (value: unknown): value is number =>
  Number.isSafeInteger(value) &&
  (value as number) >= 1 &&
  (value as number) <= REMAINING_MAX

function pseudo(plan: Plan, language = "en"): PreparedAction {
  return {
    operationId: BATCH_PREPARE_OPERATION,
    summary: batchWords(language).summary(plan.actions.length),
    write: true,
    idempotent: false,
    method: "POST",
    path: "/assistant/batches",
    body: plan,
  }
}
function planOf(review: ReturnType<typeof verifyReview>): Plan {
  const plan = review.action.body as Plan
  if (
    review.action.operationId !== BATCH_PREPARE_OPERATION ||
    !plan ||
    plan.version !== BATCH_PROTOCOL_VERSION ||
    !Array.isArray(plan.actions) ||
    plan.actions.length < 2 ||
    plan.actions.length > IMPORT_PAGE ||
    !Array.isArray(plan.actionIds) ||
    plan.actionIds.length !== plan.actions.length ||
    typeof plan.fingerprint !== "string" ||
    (plan.remaining !== undefined && !isRemaining(plan.remaining)) ||
    !Array.isArray(plan.transports) ||
    plan.transports.length !== plan.actions.length
  )
    throw new VoiceError("Invalid fixed batch review.", 403)
  return plan
}
function scopedFiles(action: PreparedAction, files: Map<string, Blob>) {
  const handles = new Set(Object.values(action.attachments ?? {}).flat())
  return new Map([...files].filter(([handle]) => handles.has(handle)))
}
function allowed(action: PreparedAction, context: VoiceContext) {
  const operation = getOperation(action.operationId)
  if (
    !operation ||
    operation.disposition !== "execute" ||
    !isToolAllowed(operation, context.permissions) ||
    !action.write
  )
    throw new VoiceError(
      "Fixed batches contain only permitted exact write operations. Use lookups and secure Office forms separately.",
      403
    )
}

/** Prepare every fully resolved request before creating one immutable Rust batch. */
export async function proposeBatch(
  calls: unknown,
  context: VoiceContext,
  files: Map<string, Blob>,
  // A price-list import page passes fifty.
  max = MODEL_BATCH_MAX,
  remaining?: unknown
): Promise<VoiceOutcome> {
  if (!Array.isArray(calls) || calls.length < 2 || calls.length > max)
    throw new VoiceError(`Choose two through ${max} exact actions.`)
  if (remaining !== undefined && !isRemaining(remaining))
    throw new VoiceError(
      `remaining must be a whole number from 1 to ${REMAINING_MAX}.`
    )
  const actions = calls.map((call: unknown) => {
    if (
      !call ||
      typeof call !== "object" ||
      !("operationId" in call) ||
      typeof call.operationId !== "string"
    )
      throw new VoiceError("Use exact operation ids and arguments.")
    const action = prepareAction(
      call.operationId,
      "arguments" in call ? call.arguments : {},
      context.workspaceId
    )
    allowed(action, context)
    return action
  })
  const handles = new Set(
    actions.flatMap((action) => Object.values(action.attachments ?? {}).flat())
  )
  if ([...files.keys()].some((handle) => !handles.has(handle)))
    throw new VoiceError("An attachment is not referenced by this batch.")
  const drafts = []
  const transports: (MultipartReview | null)[] = []
  for (const action of actions) {
    const transport = await actionTransport(
      action,
      context,
      scopedFiles(action, files)
    )
    drafts.push(businessDraft(action, transport))
    transports.push(transport?.reviewMetadata ?? null)
  }
  const stored = await assistantRequest<StoredBatch>(
    context,
    "/batches",
    "POST",
    { session_id: context.sessionId, actions: drafts },
    true
  )
  try {
    return await batchReview(stored, actions, transports, context, remaining)
  } catch (error) {
    await assistantRequest(context, `/batches/${stored.id}/cancel`, "POST", {
      session_id: context.sessionId,
      reason: "cancelled",
    }).catch(() => undefined)
    throw error
  }
}

async function batchReview(
  stored: StoredBatch,
  actions: PreparedAction[],
  transports: (MultipartReview | null)[],
  context: VoiceContext,
  remaining?: number
): Promise<VoiceOutcome> {
  if (stored.steps.length !== actions.length)
    throw new VoiceError("Fixed batch membership changed.", 409)
  const steps: VoiceReviewStep[] = []
  for (let index = 0; index < actions.length; index++) {
    allowed(actions[index]!, context)
    if (stored.steps[index]!.operation_id !== actions[index]!.operationId)
      throw new VoiceError("Fixed batch order changed.", 409)
    steps.push(
      await reviewStep(
        actions[index]!,
        context,
        stored.steps[index]!,
        transports[index] ?? undefined
      )
    )
  }
  const plan: Plan = {
    version: 1,
    fingerprint: stored.fingerprint,
    actionIds: stored.steps.map((step) => step.id),
    actions,
    transports,
    ...(remaining ? { remaining } : {}),
  }
  const action = pseudo(plan, context.language)
  await checkpointReview(action, context, stored.id, plan.actionIds)
  const secret = await voiceSetting("ASSISTANT_BRIDGE_SECRET")
  if (secret.length < 32)
    throw new VoiceError("Assistant reviews are not configured.", 503)
  const ticket = createReview(
    action,
    context,
    secret,
    context.sessionId!,
    stored.id,
    stored.expires_at
  )
  const code = verifyReview(ticket, context, secret).confirmationCode
  const attachments: Record<string, string[]> = {}
  for (const item of actions)
    for (const [field, handles] of Object.entries(item.attachments ?? {}))
      attachments[field] = [
        ...new Set([...(attachments[field] ?? []), ...handles]),
      ]
  return {
    status: "requires_confirmation",
    data: {
      batchId: stored.id,
      actionIds: plan.actionIds,
      ...(remaining ? { remaining } : {}),
    },
    review: {
      kind: "batch",
      batchId: stored.id,
      ticket,
      summary: action.summary,
      confirmationCode: code,
      confirmationPhrase: confirmationPhrase(
        code,
        context.language ?? "en",
        "batch"
      )!,
      target: batchWords(context.language).target,
      details: {
        actionCount: actions.length,
        ordered: true,
        ...(remaining ? { remaining } : {}),
      },
      steps,
      expiresAt: stored.expires_at,
      ...(Object.keys(attachments).length ? { attachments } : {}),
    },
  }
}

/** Restore the same unstarted finite review; paused work never silently rebases or resumes. */
export async function restoreBatch(
  stored: StoredBatch,
  context: VoiceContext
): Promise<VoiceOutcome> {
  if (
    !stored.recoverable ||
    !["proposed", "approved"].includes(stored.state) ||
    Date.parse(stored.expires_at) <= Date.now()
  )
    throw new VoiceError(
      "This batch needs a fresh exact review after checking its recorded outcomes.",
      409
    )
  const actions = stored.steps.map((step) => restoreAction(step))
  if (actions.some((action) => !action))
    throw new VoiceError(
      "Original batch attachment bytes are required. Check the old batch before preparing it again.",
      409
    )
  return batchReview(
    stored,
    actions as PreparedAction[],
    actions.map(() => null),
    context
  )
}

/** Trusted UI proof is independent of any model tool; whole fingerprint and order stay bound. */
export async function confirmBatch(
  review: ReturnType<typeof verifyReview>,
  input: Record<string, unknown>,
  context: VoiceContext,
  files: Map<string, Blob>,
  signal: AbortSignal,
  refresh: () => Promise<VoiceContext>
): Promise<VoiceOutcome> {
  const plan = planOf(review)
  const base = `/batches/${encodeURIComponent(review.id)}`
  const stored = await assistantRequest<StoredBatch>(context, base)
  if (
    stored.id !== review.id ||
    stored.session_id !== context.sessionId ||
    stored.fingerprint !== plan.fingerprint ||
    stored.steps.length !== plan.actionIds.length ||
    stored.steps.some(
      (step, index) =>
        step.id !== plan.actionIds[index] ||
        step.operation_id !== plan.actions[index]?.operationId
    )
  )
    throw new VoiceError(
      "This review does not match its immutable fixed batch.",
      403
    )
  if (input.decision === "cancel") {
    const cancelled = await assistantRequest<StoredBatch>(
      context,
      `${base}/cancel`,
      "POST",
      { session_id: context.sessionId, reason: "cancelled" }
    )
    if (["proposed", "approved"].includes(stored.state))
      await cancelReview(context, stored.id)
    return batchStatus(cancelled, context.language)
  }
  if (!["proposed", "approved"].includes(stored.state))
    return batchStatus(stored, context.language)
  if (
    input.decision !== "approve" ||
    !["click", "speech"].includes(String(input.method))
  )
    throw new VoiceError("Confirm the exact fixed batch in Office.")
  let event: string | undefined
  if (input.method === "speech") {
    if (
      typeof input.eventId !== "string" ||
      !/^\d{1,20}$/.test(input.eventId) ||
      typeof input.utterance !== "string" ||
      input.locale !== context.language ||
      input.confirmationCode !== review.confirmationCode ||
      !matchesSpokenConfirmation(
        input.utterance,
        review.confirmationCode,
        context.language ?? "en",
        "batch"
      )
    )
      throw new VoiceError(
        "Say the exact fixed batch confirmation phrase, or use its button."
      )
    event = createHash("sha256")
      .update(context.sessionId + ":" + input.eventId)
      .digest("hex")
  }
  for (let index = 0; index < plan.actions.length; index++) {
    const action = plan.actions[index]!
    allowed(action, context)
    const transport = await actionTransport(
      action,
      context,
      scopedFiles(action, files),
      plan.transports[index]?.boundary
    )
    if (
      (transport || plan.transports[index]) &&
      JSON.stringify(transport?.reviewMetadata) !==
        JSON.stringify(plan.transports[index])
    )
      throw new VoiceError(
        "Batch attachment content changed. Prepare a new exact review.",
        409
      )
  }
  if (signal.aborted) {
    const paused = await assistantRequest<StoredBatch>(
      context,
      `${base}/pause`,
      "POST",
      { session_id: context.sessionId, reason: "disconnected" }
    )
    return batchStatus(paused, context.language)
  }
  await assistantRequest(
    context,
    `${base}/approve`,
    "POST",
    {
      session_id: context.sessionId,
      ...(event ? { approval_event_sha256: event } : {}),
    },
    true
  )
  const outcome = await executeAction(
    review.action,
    context,
    stored.id,
    async () =>
      executeBatchController({
        batch: stored,
        actions: plan.actions,
        transports: plan.transports.map((value) => value ?? undefined),
        context,
        files,
        signal,
        refresh,
      })
  )
  return nextPart(outcome, plan.remaining, context.language)
}

/** Only a fully completed part offers the next one; a stopped part never does. */
export function nextPart(
  outcome: VoiceOutcome,
  remaining: number | undefined,
  language = "en"
): VoiceOutcome {
  if (!remaining || outcome.status !== "completed") return outcome
  const data =
    outcome.data &&
    typeof outcome.data === "object" &&
    !Array.isArray(outcome.data)
      ? outcome.data
      : {}
  return {
    ...outcome,
    data: { ...data, next: { remaining } },
    narration: batchWords(language).next(remaining),
  }
}

export { batchStatus, batchFailure } from "./batch-controller"
export type { StoredBatch }
