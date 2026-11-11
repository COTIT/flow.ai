import "server-only"
import { createHash } from "node:crypto"
import { assistantRequest } from "./bridge"
import { getOperation, prepareAction, type PreparedAction } from "./catalog"
import { serializeMultipart } from "./multipart"
import { VoiceError, type VoiceContext } from "./server"
import { voiceSetting } from "./settings"
import { checkpointReview, createReview, verifyReview } from "./workflow"
import { confirmationPhrase } from "@genesy/voice/lib/confirmation"
import { withVoiceTracing } from "./tracing"
import { workspaceTimeZone } from "./time-zone"
import { reviewTarget } from "./review-target"
import {
  createdFacts,
  reviewChanges,
  reviewFacts,
  reviewWarnings,
} from "./review-facts"
import type {
  VoiceOutcome,
  VoiceReview,
  VoiceReviewStep,
} from "@genesy/voice/lib/client-contract"
import type { MultipartReview } from "./multipart"

export type StoredAction = {
  id: string
  session_id: string
  operation_id: string
  state: string
  expires_at: string
  http_status?: number
  evidence?: unknown
  review_facts?: unknown
  batch_id?: string | null
  batch_index?: number | null
  batch_verdict?: string | null
  proposal?: {
    method: string
    path: string
    query?: string | null
    body?: unknown
    content_type?: string
    body_sha256?: string
  } | null
}

/** Bind original browser bytes to session-owned provider upload metadata. */
export async function actionTransport(
  action: PreparedAction,
  context: VoiceContext,
  files: Map<string, Blob>,
  boundary?: string
) {
  if (!action.attachments) {
    if (files.size)
      throw new VoiceError("This operation does not accept attachments.")
    return undefined
  }
  const handles = new Set(Object.values(action.attachments).flat())
  for (const handle of handles) {
    const blob = files.get(handle)
    if (!blob)
      throw new VoiceError("Send the original session attachment again.")
    const stored = await assistantRequest<{
      state: string
      file_id?: string
      size_bytes: number
      sha256?: string
    }>(
      context,
      `/sessions/${context.sessionId}/uploads/${encodeURIComponent(handle)}`
    )
    const hash = createHash("sha256")
      .update(new Uint8Array(await blob.arrayBuffer()))
      .digest("hex")
    if (
      stored.state !== "uploaded" ||
      !stored.file_id ||
      stored.size_bytes !== blob.size ||
      !stored.sha256 ||
      stored.sha256 !== hash
    )
      throw new VoiceError(
        "Attachment changed or belongs to another session. Share it again.",
        409
      )
  }
  try {
    return await serializeMultipart(action, files, boundary)
  } catch (error) {
    throw new VoiceError(
      error instanceof Error ? error.message : "Invalid business attachment."
    )
  }
}

/** Persist business draft and graph interrupt before returning the canonical review. */
export async function proposeAction(
  action: PreparedAction,
  context: VoiceContext,
  files: Map<string, Blob>
): Promise<VoiceOutcome> {
  const transport = await actionTransport(action, context, files)
  const value = {
    session_id: context.sessionId,
    ...businessDraft(action, transport),
  }
  const stored = await withVoiceTracing("propose", () =>
    assistantRequest<StoredAction>(
      context,
      "/actions",
      "POST",
      value,
      Boolean(transport)
    )
  )
  return reviewAction(action, context, stored, transport?.reviewMetadata)
}

/** Encode only native business request fields; batch/session binding stays outside it. */
export function businessDraft(
  action: PreparedAction,
  transport?: Awaited<ReturnType<typeof actionTransport>>
) {
  const url = new URL(action.path, "https://genesis.invalid")
  return {
    operation_id: action.operationId,
    method: action.method.toUpperCase(),
    path: url.pathname,
    ...(url.search ? { query: url.search.slice(1) } : {}),
    ...(transport
      ? {
          body: {
            fields: action.body ?? {},
            attachments: action.attachments,
            multipart: transport.reviewMetadata,
          },
          body_sha256: transport.reviewMetadata.bodySha256,
          content_type: transport.contentType,
        }
      : action.body !== undefined
        ? { body: action.body }
        : {}),
  }
}

/** Reissue only the same live ledger proposal; no new business action is created. */
export async function restoreReview(
  stored: StoredAction,
  context: VoiceContext
): Promise<VoiceOutcome> {
  if (stored.batch_id)
    throw new VoiceError("Review this action as part of its fixed batch.", 409)
  if (
    !["proposed", "approved"].includes(stored.state) ||
    !Number.isFinite(Date.parse(stored.expires_at)) ||
    Date.parse(stored.expires_at) <= Date.now()
  )
    throw new VoiceError(
      "This review expired or already finished. Check its recorded status.",
      409
    )
  const action = restoreAction(stored)
  if (!action)
    throw new VoiceError(
      "This review needs the original file. Prepare it again after checking its status.",
      409
    )
  return reviewAction(action, context, stored)
}

async function reviewAction(
  action: PreparedAction,
  context: VoiceContext,
  stored: StoredAction,
  multipart?: MultipartReview
): Promise<VoiceOutcome> {
  const step = await reviewStep(action, context, stored, multipart)
  await checkpointReview(action, context, stored.id)
  const secret = await voiceSetting("ASSISTANT_BRIDGE_SECRET")
  if (secret.length < 32)
    throw new VoiceError("Assistant reviews are not configured.", 503)
  const { ticket, code } = await withVoiceTracing("review_ticket", async () => {
    const ticket = createReview(
      action,
      context,
      secret,
      context.sessionId!,
      stored.id,
      stored.expires_at,
      multipart
    )
    return {
      ticket,
      code: verifyReview(ticket, context, secret).confirmationCode,
    }
  })
  const review: VoiceReview = {
    operationId: step.operationId,
    method: step.method,
    ticket,
    confirmationCode: code,
    confirmationPhrase: confirmationPhrase(code, context.language ?? "en")!,
    summary: step.summary,
    target: step.target,
    details: step.details,
    expiresAt: stored.expires_at,
    ...(action.attachments ? { attachments: action.attachments } : {}),
  }
  return {
    status: "requires_confirmation",
    review,
    data: { actionId: stored.id },
  }
}

/** Render one permission-projected review step without creating its own approval ticket. */
export async function reviewStep(
  action: PreparedAction,
  context: VoiceContext,
  stored: StoredAction,
  multipart?: MultipartReview
): Promise<VoiceReviewStep> {
  const url = new URL(action.path, "https://genesis.invalid")
  const target = await withVoiceTracing("review_target", () =>
    reviewTarget(action, context)
  )
  const projected =
    reviewFacts(
      stored.review_facts,
      action.operationId,
      context.language ?? "en"
    ) ?? createdFacts(action.operationId, action.body, context.language ?? "en")
  // Impact sits beside the record on the card; the API's reviewed label names it when lookup could not.
  const { impact, ...facts } = projected ?? {}
  const locked = (projected?.locked ?? {}) as { label?: unknown }
  const record = target.record as { label: unknown } | undefined
  if (record && record.label == null && typeof locked.label === "string")
    record.label = locked.label
  const warnings = reviewWarnings(stored.review_facts, context.language ?? "en")
  return {
    actionId: stored.id,
    operationId: action.operationId,
    method: action.method.toUpperCase(),
    summary: action.summary,
    target: action.operationId.split("_").slice(0, -1).join(" "),
    details: {
      ...target,
      ...(warnings.length ? { warnings } : {}),
      // A delete's record and impact say it all; its locked snapshot would only repeat the id.
      ...(impact ? { impact } : projected ? { current: facts } : {}),
      ...(action.body !== undefined
        ? {
            changes: reviewChanges(
              action.body,
              stored.review_facts,
              action.operationId,
              context.language ?? "en",
              action.operationId === "scheduled_reports_create"
                ? await workspaceTimeZone(context)
                : undefined
            ),
          }
        : {}),
      ...(url.search
        ? { selection: Object.fromEntries(url.searchParams) }
        : {}),
      ...(multipart
        ? {
            attachments: multipart.files.map(({ field, name, size }) => ({
              field,
              name,
              size,
            })),
          }
        : {}),
    },
  }
}

/** Safe status projection never equates a lost acknowledgement with a rejected write. */
export function actionStatus(
  stored: StoredAction,
  active = false
): VoiceOutcome {
  const status =
    stored.state === "dispatching"
      ? "running"
      : stored.state === "proposed" || stored.state === "approved"
        ? "awaiting_review"
        : stored.state
  return {
    status,
    data: {
      actionId: stored.id,
      ...(stored.batch_id ? { batchId: stored.batch_id } : {}),
      operation: stored.operation_id,
      summary:
        getOperation(stored.operation_id)?.summary ?? stored.operation_id,
      recoverable:
        active &&
        !stored.batch_id &&
        status === "awaiting_review" &&
        Date.parse(stored.expires_at) > Date.now() &&
        Boolean(stored.proposal) &&
        !stored.proposal?.content_type?.startsWith("multipart/"),
      httpStatus: stored.http_status,
      evidence: stored.evidence,
    },
    ...(status === "unknown"
      ? {
          message:
            "Completion is uncertain. Check the business record before starting another action.",
        }
      : {}),
  }
}

/** Reconstruct an owned unexpired JSON proposal; multipart originals still require re-upload. */
export function restoreAction(
  stored: StoredAction
): PreparedAction | undefined {
  if (
    !stored.proposal ||
    stored.proposal.content_type?.startsWith("multipart/")
  )
    return undefined
  const tool = getOperation(stored.operation_id)
  if (!tool || tool.disposition !== "execute") return undefined
  const url = new URL(stored.proposal.path, "https://genesis.invalid")
  if (stored.proposal.query) url.search = stored.proposal.query
  const path: Record<string, string> = {}
  const template = tool.path.split("/")
  const actual = url.pathname.replace(/^\/v1(?=\/)/, "").split("/")
  template.forEach((part, i) => {
    if (part.startsWith("{") && part !== "{workspace}")
      path[part.slice(1, -1)] = decodeURIComponent(actual[i] ?? "")
  })
  const workspace = actual[template.indexOf("{workspace}")]
  return prepareAction(
    tool.id,
    {
      path,
      query: Object.fromEntries(url.searchParams),
      ...(stored.proposal.body !== undefined && stored.proposal.body !== null
        ? { body: stored.proposal.body }
        : {}),
    },
    workspace ?? ""
  )
}
