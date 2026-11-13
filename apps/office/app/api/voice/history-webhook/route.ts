import { readBounded } from "@/lib/voice/bridge"
import { ingestProviderTranscript } from "@/lib/voice/history"
import {
  providerTranscript,
  verifyTranscriptSignature,
} from "@/lib/voice/history-transcript"
import { voiceSetting } from "@/lib/voice/settings"
import { VoiceError, voiceFailure, voiceResponse } from "@/lib/voice/server"

export const runtime = "nodejs"

/** Native provider receipt is HMAC-authenticated and acknowledged only after durable commit. */
export async function POST(request: Request) {
  try {
    if (!request.headers.get("content-type")?.startsWith("application/json"))
      throw new VoiceError("Expected a JSON transcript.", 415)
    const body = await readBounded(request.body, 2 * 1024 * 1024)
    const [secret, agent] = await Promise.all([
      voiceSetting("ELEVENLABS_WEBHOOK_SECRET"),
      voiceSetting("ELEVENLABS_AGENT_ID"),
    ])
    if (!secret || !agent)
      throw new VoiceError("Transcript receiver is not configured.", 503)
    verifyTranscriptSignature(
      body,
      request.headers.get("elevenlabs-signature"),
      secret
    )
    let payload: { type?: unknown; data?: unknown }
    try {
      payload = JSON.parse(
        new TextDecoder("utf-8", { fatal: true }).decode(body)
      )
    } catch {
      throw new VoiceError("Invalid transcript payload.")
    }
    if (payload.type !== "post_call_transcription")
      return voiceResponse({ status: "ignored" })
    return voiceResponse(
      await ingestProviderTranscript(providerTranscript(payload.data, agent))
    )
  } catch (error) {
    return voiceFailure(error)
  }
}
