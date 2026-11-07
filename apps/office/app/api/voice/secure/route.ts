import { randomUUID } from "node:crypto"
import {
  authorizeVoice,
  callBackend,
  VoiceError,
  voiceFailure,
  voiceResponse,
} from "@/lib/voice/server"
import { assistantRequest, type AssistantSession } from "@/lib/voice/bridge"
import {
  getOperation,
  isToolAllowed,
  prepareAction,
  prepareSecureAction,
} from "@/lib/voice/catalog"
import { voiceInput } from "@/lib/voice/input"
import { requireAssistantAdmission } from "@/lib/voice/admission"
import { settingsFormValues } from "@/lib/voice/settings-form"
export const runtime = "nodejs"

/** Native secure form only; neither arguments nor results return to conversation or traces. */
export async function POST(request: Request) {
  try {
    await requireAssistantAdmission()
    if (request.headers.get("content-type")?.startsWith("multipart/"))
      throw new VoiceError("Use the native business file picker.")
    const { input, files } = await voiceInput(request)
    if (files.size) throw new VoiceError("Use the native business file picker.")
    const context = await authorizeVoice(request, input.workspaceId)
    await requireAssistantAdmission(context.workspaceId)
    if (input.sessionId) {
      if (
        typeof input.sessionId !== "string" ||
        !/^[a-zA-Z0-9-]{1,100}$/.test(input.sessionId)
      )
        throw new VoiceError("Invalid session.")
      const session = await assistantRequest<AssistantSession>(
        context,
        "/sessions/" + input.sessionId
      )
      if (session.state === "active")
        throw new VoiceError(
          "End the conversation before entering confidential values.",
          409
        )
    }
    const operation = getOperation(String(input.operationId ?? ""))
    if (
      !operation ||
      operation.disposition !== "secure_ui" ||
      !isToolAllowed(operation, context.permissions)
    )
      throw new VoiceError("This secure operation is not permitted.", 403)
    if (input.command === "describe")
      return voiceResponse({
        id: operation.id,
        summary: operation.summary,
        parameters: operation.parameters,
        body: operation.body,
      })
    if (input.command !== "execute")
      throw new VoiceError("Unknown secure form command.")
    let action
    try {
      action = prepareSecureAction(
        operation.id,
        input.arguments ?? {},
        context.workspaceId
      )
    } catch (error) {
      throw new VoiceError(
        error instanceof Error ? error.message : "Invalid secure form."
      )
    }
    context.nativeSecure = true
    if (operation.id === "settings_update") {
      const current = await callBackend(
        prepareAction("settings_get", {}, context.workspaceId),
        context,
        randomUUID()
      )
      if (
        current.status !== "completed" ||
        !current.data ||
        typeof current.data !== "object" ||
        !("data" in current.data)
      )
        throw new VoiceError(
          "Current settings could not be loaded. Nothing was submitted.",
          503
        )
      action = {
        ...action,
        body: settingsFormValues(current.data.data, action.body),
      }
    }
    return voiceResponse(await callBackend(action, context, randomUUID()))
  } catch (error) {
    return voiceFailure(error)
  }
}
