import { VoiceError, voiceResponse } from "@/lib/voice/server"
import { assistantRequest } from "@/lib/voice/bridge"
import { authorizeVoiceSession } from "@/lib/voice/identity"
import {
  batchFailure,
  proposeBatch,
  restoreBatch,
  batchStatus,
  type StoredBatch,
} from "@/lib/voice/batches"
import { takeFilePermit, voiceInput } from "@/lib/voice/input"
import { requireAssistantAdmission } from "@/lib/voice/admission"
import { cancelReview } from "@/lib/voice/workflow"
import { withVoiceTracing } from "@/lib/voice/tracing"
export const runtime = "nodejs"

/** Model can prepare/read; only independent UI confirmation starts a fixed batch. */
export function POST(request: Request) {
  return withVoiceTracing("genesy_assistant_batch", () => handle(request))
}
async function handle(request: Request) {
  let release: (() => void) | undefined
  let language = "en"
  try {
    release = takeFilePermit(request)
    const { input, files } = await voiceInput(request)
    const command = String(input.command ?? "")
    if (
      !["prepare", "restore", "status", "list", "pause", "cancel"].includes(
        command
      )
    )
      throw new VoiceError("Unknown fixed batch command.")
    if (["prepare", "restore"].includes(command))
      await requireAssistantAdmission()
    const { context, session } = await authorizeVoiceSession(
      request,
      input.workspaceId,
      input.sessionId,
      ["prepare", "restore"].includes(command) ? "live" : "read"
    )
    if (["prepare", "restore"].includes(command))
      await requireAssistantAdmission(context.workspaceId)
    language = context.language ?? "en"
    if (command === "prepare") {
      if (!session.conversation_id)
        throw new VoiceError("Conversation identity has not been bound.", 409)
      return voiceResponse(await proposeBatch(input.calls, context, files))
    }
    if (files.size)
      throw new VoiceError(
        "Batch status and stop commands do not accept files."
      )
    if (command === "list") {
      const rows = await assistantRequest<StoredBatch[]>(
        context,
        `/sessions/${session.id}/batches`
      )
      return voiceResponse({
        status: "completed",
        data: rows.map((row) => batchStatus(row, context.language)),
      })
    }
    if (
      typeof input.batchId !== "string" ||
      !/^[0-9a-f-]{36}$/i.test(input.batchId)
    )
      throw new VoiceError("Choose a fixed batch reference.")
    const base = `/batches/${encodeURIComponent(input.batchId)}`
    const stored = await assistantRequest<StoredBatch>(context, base)
    if (stored.session_id !== session.id)
      throw new VoiceError("This fixed batch belongs to another session.", 403)
    if (command === "restore")
      return voiceResponse(await restoreBatch(stored, context))
    if (command === "pause" || command === "cancel") {
      const reason =
        command === "cancel"
          ? "cancelled"
          : ["disconnected", "changed_intent", "interrupted"].includes(
                String(input.reason)
              )
            ? String(input.reason)
            : "disconnected"
      const stopped = await assistantRequest<StoredBatch>(
        context,
        `${base}/${command}`,
        "POST",
        { session_id: session.id, reason }
      )
      if (
        command === "cancel" &&
        ["proposed", "approved"].includes(stored.state)
      )
        await cancelReview(context, stored.id)
      return voiceResponse(batchStatus(stopped, context.language))
    }
    return voiceResponse(batchStatus(stored, context.language))
  } catch (error) {
    return batchFailure(error, language)
  } finally {
    release?.()
  }
}
