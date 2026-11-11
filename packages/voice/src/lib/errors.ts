/** No usable HTTP answer: the browser was offline, the line dropped, or the body was unreadable. */
export type VoiceFailureKind = "offline" | "network" | "bad_response"

/** A voice BFF call failed. With `kind`, no HTTP answer arrived (status 0); otherwise `message` is translated text. */
export class VoiceRequestError extends Error {
  constructor(
    message: string,
    readonly status: number,
    /** Model-only retry hints: an exact lookup schema or candidate operations. */
    readonly hint?: {
      message?: string
      operation?: unknown
      candidates?: unknown
      reason?: string
    },
    readonly kind?: VoiceFailureKind
  ) {
    super(message)
  }
}

/** Genesy's own failure whose message is already translated text, safe to show. */
export class VoiceShownError extends Error {}

const DEVICE_KEYS: Record<string, string> = {
  NotAllowedError: "microphoneDenied",
  SecurityError: "microphoneDenied",
  NotFoundError: "microphoneMissing",
  OverconstrainedError: "microphoneMissing",
}

/**
 * The human text for any failure: never the browser's or a library's own message.
 * `lost` is the line for a dropped or garbled answer; a write passes "requestErrors.unverified".
 */
export function voiceErrorMessage(
  e: unknown,
  t: (key: string) => string,
  fallback = "requestFailed",
  lost = "connectionLost"
) {
  if (e instanceof VoiceRequestError)
    return e.kind ? t(e.kind === "offline" ? "offline" : lost) : e.message
  if (e instanceof VoiceShownError) return e.message
  const device = e instanceof Error ? DEVICE_KEYS[e.name] : undefined
  return t(device ?? fallback)
}
