/** No usable HTTP answer: offline (nothing was sent), the line dropped (maybe sent), or an unreadable body. */
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
  const device =
    e instanceof Error || e instanceof DOMException
      ? DEVICE_KEYS[e.name]
      : undefined
  return t(device ?? fallback)
}

/** Back online, the offline line no longer holds (J3); any other error stays as it is. */
export const errorAfterOnline = (error: string, offline: string) =>
  error === offline ? "" : error
