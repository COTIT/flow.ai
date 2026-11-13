import {
  authorizeVoice,
  VoiceError,
  voiceFailure,
  voiceResponse,
} from "@/lib/voice/server"
import {
  assistantRequest,
  requireAssistantSession,
  type AssistantSession,
} from "@/lib/voice/bridge"
import { signedProviderSession } from "@/lib/voice/provider"
import { voiceInput } from "@/lib/voice/input"
import { requireAssistantAdmission } from "@/lib/voice/admission"
import { assistantReadiness } from "@/lib/voice/readiness"
import { workspaceTimeZone } from "@/lib/voice/time-zone"
import {
  GREETING_HINTS,
  sessionGreeting,
  type GreetingHint,
} from "@/lib/voice/greeting"
import { noteVoiceFailure, withVoiceTracing } from "@/lib/voice/tracing"
import { prepareConversation } from "@/lib/voice/history"
import { voiceSetting } from "@/lib/voice/settings"
export const runtime = "nodejs"

/** Allocate and bind provider identity before returning signed access to the browser. */
export async function POST(request: Request) {
  try {
    await requireAssistantAdmission()
    if (request.headers.get("content-type")?.startsWith("multipart/"))
      throw new VoiceError("Session requests do not accept files.")
    const { input, files } = await voiceInput(request)
    if (files.size)
      throw new VoiceError("Session requests do not accept files.")
    const context = await authorizeVoice(request, input.workspaceId)
    await requireAssistantAdmission(context.workspaceId)
    const language = input.locale ?? input.language ?? "en"
    if (typeof language !== "string" || !["en", "nl", "sw"].includes(language))
      throw new VoiceError("Choose English, Dutch or Swahili.")
    if (input.command === "heartbeat") {
      await requireAssistantSession(context, input.sessionId)
      await assistantRequest(
        context,
        "/sessions/" +
          encodeURIComponent(String(input.sessionId)) +
          "/heartbeat",
        "POST"
      )
      return voiceResponse({ status: "active" })
    }
    if (input.command === "dropped") {
      // The provider closed the conversation on its own: record it for review, never as completed.
      const kind = String(input.endedBy)
      if (!["ended_by_agent", "time_limit", "connection_lost"].includes(kind))
        throw new VoiceError("Invalid end reason.")
      await requireAssistantSession(context, input.sessionId)
      return voiceResponse(
        await withVoiceTracing("genesy_assistant_dropped", async () => {
          noteVoiceFailure(`dropped_${kind}`)
          return { status: "recorded" }
        })
      )
    }
    const hint = input.greeting as GreetingHint | undefined
    if (hint !== undefined && !GREETING_HINTS.includes(hint))
      throw new VoiceError("Invalid greeting.")
    if (!(await assistantReadiness()).ready)
      throw new VoiceError("Assistant configuration is not ready.", 503)
    const history =
      input.history === true
        ? await prepareConversation(context, input.logicalConversationId)
        : undefined
    if (input.logicalConversationId !== undefined && !history)
      throw new VoiceError("Conversation history must be explicitly enabled.")
    if (history && !context.userId)
      throw new VoiceError("Conversation ownership could not be verified.", 503)
    const zone = workspaceTimeZone(context)
    const [timeZone, greeting] = await Promise.all([
      zone,
      zone.then((found) => sessionGreeting(context, language, hint, found)),
    ])
    const session = await assistantRequest<AssistantSession>(
      context,
      "/sessions",
      "POST",
      { language }
    )
    try {
      const provider = await signedProviderSession()
      await assistantRequest(
        context,
        "/sessions/" + session.id + "/attach",
        "POST",
        { conversation_id: provider.conversationId },
        true
      )
      const source = history
        ? await assistantRequest<{ id: string }>(
            context,
            `/conversations/${history.conversation.id}/sources`,
            "POST",
            {
              session_id: session.id,
              provider_conversation_id: provider.conversationId,
              agent_id: await voiceSetting("ELEVENLABS_AGENT_ID"),
              text_only: input.textOnly === true,
            },
            true
          )
        : undefined
      return voiceResponse({
        ...provider,
        sessionId: session.id,
        ...(timeZone ? { timeZone } : {}),
        ...(greeting !== undefined ? { greeting } : {}),
        ...(history && source
          ? {
              ...(input.logicalConversationId ? { greeting: "" } : {}),
              history: {
                conversationId: history.conversation.id,
                sourceId: source.id,
                userId: context.userId,
                context: history.context,
                revision: history.conversation.revision,
                coverage: history.conversation.coverage,
              },
            }
          : {}),
      })
    } catch (error) {
      await assistantRequest(
        context,
        "/sessions/" + session.id,
        "DELETE"
      ).catch(() => undefined)
      throw error
    }
  } catch (error) {
    return voiceFailure(error)
  }
}

/** Closing schedules durable provider cleanup; business outcomes remain in Rust audit. */
export async function DELETE(request: Request) {
  try {
    if (request.headers.get("content-type")?.startsWith("multipart/"))
      throw new VoiceError("Session requests do not accept files.")
    const { input, files } = await voiceInput(request)
    if (files.size)
      throw new VoiceError("Session requests do not accept files.")
    const context = await authorizeVoice(request, input.workspaceId)
    if (
      typeof input.sessionId !== "string" ||
      !/^[a-zA-Z0-9-]{1,100}$/.test(input.sessionId)
    )
      throw new VoiceError("Invalid assistant session.")
    await assistantRequest(
      context,
      "/sessions/" + encodeURIComponent(input.sessionId),
      "DELETE"
    )
    return voiceResponse({ status: "ended" })
  } catch (error) {
    return voiceFailure(error)
  }
}
