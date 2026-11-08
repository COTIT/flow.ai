import "server-only"
import { createHash, createHmac } from "node:crypto"
import { VoiceError, type VoiceContext } from "./server"
import { voiceSetting } from "./settings"

/** Authenticate exact privileged bridge requests; callers still supply ordinary staff auth. */
export async function bridgeHeaders(
  method: string,
  path: string,
  body: string,
  timestamp = Math.floor(Date.now() / 1000)
): Promise<Record<string, string>> {
  const secret = await voiceSetting("ASSISTANT_BRIDGE_SECRET")
  if (secret.length < 32)
    throw new VoiceError(
      "Assistant server authentication is not configured.",
      503
    )
  const digest = createHash("sha256").update(body).digest("hex")
  return {
    "Assistant-Bridge-Timestamp": String(timestamp),
    "Assistant-Bridge-Signature": createHmac("sha256", secret)
      .update(`${timestamp}\n${method}\n${path}\n${digest}`)
      .digest("hex"),
  }
}

/** Read a bounded response or upload even when Content-Length is absent or forged. */
export async function readBounded(
  stream: ReadableStream<Uint8Array> | null,
  limit: number,
  timeoutMs = 15000
): Promise<Uint8Array> {
  if (!stream) return new Uint8Array()
  const reader = stream.getReader()
  const chunks: Uint8Array[] = []
  let length = 0
  let timedOut = false
  const timer = setTimeout(() => {
    timedOut = true
    void reader.cancel().catch(() => undefined)
  }, timeoutMs)
  try {
    for (;;) {
      const { value, done } = await reader.read()
      if (timedOut)
        throw new VoiceError("The request body arrived too slowly.", 408)
      if (done) break
      length += value.byteLength
      if (length > limit) {
        await reader.cancel()
        throw new VoiceError("This file or response is too large.", 413)
      }
      chunks.push(value)
    }
  } finally {
    clearTimeout(timer)
    reader.releaseLock()
  }
  const output = new Uint8Array(length)
  let offset = 0
  for (const chunk of chunks) {
    output.set(chunk, offset)
    offset += chunk.byteLength
  }
  return output
}

/** Shared per-request backend bridge; never reconfigures the generated global API client. */
export async function assistantRequest<T = Record<string, unknown>>(
  context: VoiceContext,
  suffix: string,
  method = "GET",
  value?: unknown,
  signed = false
): Promise<T> {
  const url = new URL(
    `${context.apiUrl}/workspace/${encodeURIComponent(context.workspaceId)}/assistant${suffix}`
  )
  const body = value === undefined ? "" : JSON.stringify(value)
  const response = await fetch(url, {
    method,
    headers: {
      Authorization: `Bearer ${context.token}`,
      Accept: "application/json",
      ...(body ? { "Content-Type": "application/json" } : {}),
      ...(signed
        ? await bridgeHeaders(method, url.pathname + url.search, body)
        : {}),
    },
    ...(body ? { body } : {}),
    cache: "no-store",
    redirect: "error",
    signal: AbortSignal.timeout(15000),
  })
  const bytes = await readBounded(response.body, 200000)
  let result: { data?: T; message?: string }
  try {
    result = bytes.length ? JSON.parse(new TextDecoder().decode(bytes)) : {}
  } catch {
    throw new VoiceError(
      "Assistant storage returned an unreadable response.",
      503
    )
  }
  if (!response.ok)
    throw new VoiceError(
      result.message || "Assistant request was refused.",
      response.status
    )
  return (result.data ?? result) as T
}

export type AssistantSession = {
  id: string
  language?: string
  conversation_id?: string | null
  state?: string
  closed_at?: string | null
  expires_at: string
}

/** Read retained status through Rust ownership and login checks, including ended sessions. */
export async function readAssistantSession(
  context: VoiceContext,
  id: unknown
): Promise<AssistantSession> {
  if (typeof id !== "string" || !/^[a-zA-Z0-9-]{1,100}$/.test(id))
    throw new VoiceError("Start an assistant session first.")
  return assistantRequest<AssistantSession>(
    context,
    `/sessions/${encodeURIComponent(id)}`
  )
}

/** Require a live owned session before any new provider or business operation. */
export async function requireAssistantSession(
  context: VoiceContext,
  id: unknown
): Promise<AssistantSession> {
  return requireLive(await readAssistantSession(context, id))
}

export function requireLive(session: AssistantSession): AssistantSession {
  if (
    session.closed_at ||
    session.state !== "active" ||
    !Number.isFinite(Date.parse(session.expires_at)) ||
    Date.parse(session.expires_at) <= Date.now()
  )
    throw new VoiceError("This assistant session has ended.", 410)
  return session
}
