import {
  authorizeVoice,
  VoiceError,
  voiceFailure,
  voiceResponse,
} from "@/lib/voice/server"
import { assistantRequest, requireAssistantSession } from "@/lib/voice/bridge"
import {
  actionStatus,
  actionTransport,
  type StoredAction,
} from "@/lib/voice/actions"
import { getOperation, isToolAllowed } from "@/lib/voice/catalog"
import { cancelReview, executeAction, verifyReview } from "@/lib/voice/workflow"
import { voiceSetting } from "@/lib/voice/settings"
import { takeFilePermit, voiceInput } from "@/lib/voice/input"
import { createHash } from "node:crypto"
import { matchesSpokenConfirmation } from "@/lib/voice/confirmation"
import { requireAssistantAdmission } from "@/lib/voice/admission"
import {
  batchFailure,
  confirmBatch,
  BATCH_PREPARE_OPERATION,
} from "@/lib/voice/batches"
import { withVoiceTracing } from "@/lib/voice/tracing"
export const runtime = "nodejs"

/** Trusted UI boundary; never registered as an ElevenLabs client tool. */
export function POST(request: Request) {
  return withVoiceTracing("genesy_assistant_confirm", () => handle(request))
}
async function handle(request: Request) {
  let releaseFile: (() => void) | undefined
  let language = "en"
  let isBatch = false
  try {
    releaseFile = takeFilePermit(request)
    const { input, files } = await voiceInput(request)
    if (input.decision !== "cancel") await requireAssistantAdmission()
    const context = await withVoiceTracing("authorize", () =>
      authorizeVoice(request, input.workspaceId)
    )
    if (input.decision !== "cancel")
      await requireAssistantAdmission(context.workspaceId)
    const session = await withVoiceTracing("session_lookup", () =>
      requireAssistantSession(context, input.sessionId)
    )
    context.sessionId = session.id
    context.language = session.language ?? "en"
    language = context.language
    const secret = await voiceSetting("ASSISTANT_BRIDGE_SECRET")
    if (secret.length < 32)
      throw new VoiceError("Assistant confirmation is not configured.", 503)
    const review = verifyReview(String(input.ticket ?? ""), context, secret)
    if (review.action.operationId === BATCH_PREPARE_OPERATION) {
      isBatch = true
      return voiceResponse(
        await confirmBatch(
          review,
          input,
          context,
          files,
          request.signal,
          async () => {
            const fresh = await authorizeVoice(request, input.workspaceId)
            await requireAssistantAdmission(fresh.workspaceId)
            const active = await requireAssistantSession(fresh, session.id)
            return {
              ...fresh,
              sessionId: active.id,
              language: active.language ?? "en",
            }
          }
        )
      )
    }
    const stored = await assistantRequest<StoredAction>(
      context,
      "/actions/" + review.id
    )
    if (
      stored.session_id !== session.id ||
      stored.operation_id !== review.action.operationId
    )
      throw new VoiceError("Review does not match this task.", 403)
    if (!["proposed", "approved"].includes(stored.state))
      return voiceResponse(actionStatus(stored))
    if (input.decision === "cancel") {
      await assistantRequest(
        context,
        "/actions/" + review.id + "/cancel",
        "POST",
        { session_id: session.id }
      )
      await cancelReview(context, review.id)
      return voiceResponse({ status: "cancelled", actionId: review.id })
    }
    if (
      input.decision !== "approve" ||
      !["click", "speech"].includes(String(input.method))
    )
      throw new VoiceError("Confirm the exact proposal in Office.")
    let approvalEvent: string | undefined
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
          context.language
        )
      )
        throw new VoiceError(
          "Say the exact confirmation phrase for this proposal, or use its button."
        )
      approvalEvent = createHash("sha256")
        .update(session.id + ":" + input.eventId)
        .digest("hex")
    }
    const operation = getOperation(review.action.operationId)
    if (
      !operation ||
      operation.disposition !== "execute" ||
      !isToolAllowed(operation, context.permissions)
    )
      throw new VoiceError("This action is no longer permitted.", 403)
    const transport = await actionTransport(
      review.action,
      context,
      files,
      review.multipart?.boundary
    )
    if (
      (review.multipart || transport) &&
      JSON.stringify(transport?.reviewMetadata) !==
        JSON.stringify(review.multipart)
    )
      throw new VoiceError(
        "Attachment content changed. Prepare a new review.",
        409
      )
    if (transport)
      context.transport = {
        bytes: transport.bytes,
        contentType: transport.contentType,
      }
    await withVoiceTracing("approve", () =>
      assistantRequest(
        context,
        "/actions/" + review.id + "/approve",
        "POST",
        {
          session_id: session.id,
          ...(approvalEvent ? { approval_event_sha256: approvalEvent } : {}),
        },
        true
      )
    )
    return voiceResponse(await executeAction(review.action, context, review.id))
  } catch (error) {
    return isBatch ? batchFailure(error, language) : voiceFailure(error)
  } finally {
    releaseFile?.()
  }
}
