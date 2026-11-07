import "server-only"
import { readBounded } from "./bridge"
import { VoiceError } from "./server"

const runtime = globalThis as typeof globalThis & {
  __genesyVoiceBinaryRequests?: { active: number }
}
const binaryRequests = (runtime.__genesyVoiceBinaryRequests ??= { active: 0 })

/** Bound per-process buffers until parsing, canonicalization and forwarding finish. */
export function takeFilePermit(request: Request, raw = false): () => void {
  if (
    !raw &&
    !request.headers.get("content-type")?.startsWith("multipart/form-data")
  )
    return () => undefined
  if (binaryRequests.active >= 2)
    throw new VoiceError("Two file requests are already being processed.", 429)
  binaryRequests.active++
  let released = false
  return () => {
    if (!released) {
      released = true
      binaryRequests.active--
    }
  }
}

/** Parse bounded JSON or session-file parts; models never send raw binary/base64 arguments. */
export async function voiceInput(
  request: Request
): Promise<{ input: Record<string, unknown>; files: Map<string, Blob> }> {
  const multipart =
    request.headers.get("content-type")?.startsWith("multipart/form-data") ===
    true
  const bytes = await readBounded(
    request.body,
    multipart ? 10 * 1024 * 1024 + 65536 : 131072
  )
  const files = new Map<string, Blob>()
  let payload: string
  if (multipart) {
    let form: FormData
    try {
      form = await new Request(request.url, {
        method: "POST",
        headers: { "Content-Type": request.headers.get("content-type")! },
        body: new Blob([bytes as Uint8Array<ArrayBuffer>]),
      }).formData()
    } catch {
      throw new VoiceError("Invalid attachment request.")
    }
    const values = form.getAll("payload")
    if (
      values.length !== 1 ||
      typeof values[0] !== "string" ||
      values[0].length > 131072
    )
      throw new VoiceError("Invalid attachment payload.")
    payload = values[0]
    for (const [name, file] of form) {
      if (name === "payload") continue
      if (
        typeof file === "string" ||
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
          name
        ) ||
        files.has(name) ||
        files.size >= 10
      )
        throw new VoiceError("Use unique session attachment handles.")
      files.set(name, file)
    }
  } else payload = new TextDecoder().decode(bytes)
  let input: unknown
  try {
    input = JSON.parse(payload)
  } catch {
    throw new VoiceError("Arguments must be valid JSON.")
  }
  if (!input || typeof input !== "object" || Array.isArray(input))
    throw new VoiceError("Arguments must be an object.")
  return { input: input as Record<string, unknown>, files }
}
