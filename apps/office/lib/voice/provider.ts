import "server-only"
import { readBounded } from "./bridge"
import { VoiceError } from "./server"
import { voiceSetting } from "./settings"

const ORIGIN = "https://api.elevenlabs.io"

/** Fixed provider origin; bytes, credentials and raw provider errors never enter traces. */
export async function providerRequest<T = Record<string, unknown>>(
  path: string,
  options: { method?: string; body?: BodyInit } = {}
): Promise<T> {
  const key = await voiceSetting("ELEVENLABS_API_KEY")
  if (!key) throw new VoiceError("ElevenLabs is not configured.", 503)
  if (!path.startsWith("/v1/convai/"))
    throw new VoiceError("Invalid provider request.")
  const response = await fetch(ORIGIN + path, {
    method: options.method ?? "GET",
    headers: { "xi-api-key": key },
    body: options.body,
    redirect: "error",
    cache: "no-store",
    signal: AbortSignal.timeout(30000),
  })
  if (!response.ok)
    throw new VoiceError(
      "ElevenLabs could not process this request. Check its status before repeating an upload.",
      response.status === 429 ? 429 : response.status === 422 ? 422 : 503
    )
  const bytes = await readBounded(response.body, 2000000)
  try {
    return (
      bytes.length ? JSON.parse(new TextDecoder().decode(bytes)) : {}
    ) as T
  } catch {
    throw new VoiceError("ElevenLabs returned an unreadable response.", 503)
  }
}

export async function signedProviderSession(): Promise<{
  signedUrl: string
  conversationId: string
}> {
  const agent = await voiceSetting("ELEVENLABS_AGENT_ID")
  if (!agent)
    throw new VoiceError("The Office assistant is not configured.", 503)
  const result = await providerRequest<{
    signed_url?: string
    conversation_id?: string
  }>(
    `/v1/convai/conversation/get-signed-url?agent_id=${encodeURIComponent(agent)}&include_conversation_id=true`
  )
  if (!result.signed_url)
    throw new VoiceError("ElevenLabs did not return a signed session.", 503)
  const url = new URL(result.signed_url)
  if (url.protocol !== "wss:" || url.hostname !== "api.elevenlabs.io")
    throw new VoiceError("Unexpected conversation endpoint.", 503)
  const allocated = url.searchParams.get("conversation_id")
  const conversationId = result.conversation_id ?? allocated
  if (
    !conversationId ||
    !/^[a-zA-Z0-9_-]{1,200}$/.test(conversationId) ||
    (result.conversation_id &&
      allocated &&
      allocated !== result.conversation_id)
  )
    throw new VoiceError(
      "ElevenLabs did not allocate a verifiable conversation. Session refused.",
      503
    )
  if (
    url.searchParams.has("agent_id") &&
    url.searchParams.get("agent_id") !== agent
  )
    throw new VoiceError("Conversation belongs to another agent.", 503)
  return { signedUrl: result.signed_url, conversationId }
}
