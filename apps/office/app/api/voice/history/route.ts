import { assistantRequest } from "@/lib/voice/bridge"
import {
  authorizeVoice,
  VoiceError,
  voiceFailure,
  voiceResponse,
} from "@/lib/voice/server"
import { voiceInput } from "@/lib/voice/input"
import {
  conversationId,
  historyQuery,
  historySourceQuery,
  requireConversationsEnabled,
} from "@/lib/voice/history"
import { archiveText } from "@/lib/voice/history-transcript"

export const runtime = "nodejs"

/** Browser history controls retain ordinary user/workspace authorization for every operation. */
export async function POST(request: Request) {
  try {
    const { input, files } = await voiceInput(request)
    if (files.size)
      throw new VoiceError("History requests do not accept files.")
    const commands = [
      "list",
      "new",
      "get",
      "messages",
      "events",
      "end",
      "delete",
    ]
    if (!commands.includes(String(input.command)))
      throw new VoiceError("Invalid history command.")
    const context = await authorizeVoice(request, input.workspaceId)
    if (input.command === "list")
      return voiceResponse(
        await assistantRequest(context, `/conversations${historyQuery(input)}`)
      )
    if (input.command === "new") {
      await requireConversationsEnabled()
      return voiceResponse(
        await assistantRequest(context, "/conversations", "POST", {})
      )
    }
    const id = conversationId(input.logicalConversationId)
    const base = `/conversations/${id}`
    if (input.command === "get")
      return voiceResponse(
        await assistantRequest(context, `${base}${historySourceQuery(input)}`)
      )
    if (input.command === "messages")
      return voiceResponse(
        await assistantRequest(
          context,
          `${base}/messages${historyQuery(input)}`
        )
      )
    if (input.command === "delete")
      return voiceResponse(await assistantRequest(context, base, "DELETE"))
    if (input.command === "end")
      return voiceResponse(
        await assistantRequest(context, `${base}/end`, "POST", {})
      )
    if (
      !Array.isArray(input.events) ||
      !input.events.length ||
      input.events.length > 100
    )
      throw new VoiceError("Invalid conversation events.")
    const events = input.events.map((event: Record<string, unknown>) => {
      if (!event || typeof event.text !== "string")
        throw new VoiceError("Invalid conversation event.")
      return {
        key: event.key,
        ordinal: event.ordinal,
        role: event.role,
        text: archiveText(event.text),
        modality: event.modality,
        status: event.status,
        interrupted: event.interrupted === true,
      }
    })
    await assistantRequest(context, `${base}/events`, "POST", {
      session_id: conversationId(input.sessionId),
      source_id: conversationId(input.sourceId),
      events,
    })
    return voiceResponse({ status: "saved" })
  } catch (error) {
    return voiceFailure(error)
  }
}
