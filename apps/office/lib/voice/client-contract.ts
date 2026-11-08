/** Shared wire shapes. An uploaded file is not proof the model inspected it. */
export type VoiceSession = {
  signedUrl: string
  sessionId: string
  conversationId: string
}
export type VoiceReviewStep = {
  actionId: string
  operationId?: string
  method?: string
  summary: string
  target?: string
  details: Record<string, unknown>
}
export type VoiceReview = {
  operationId?: string
  method?: string
  kind?: "batch"
  batchId?: string
  steps?: VoiceReviewStep[]
  ticket: string
  summary: string
  confirmationCode: string
  confirmationPhrase: string
  target?: string
  details: Record<string, unknown>
  expiresAt: string
  attachments?: Record<string, string[]>
}
export type VoiceOutcome = {
  status: string
  message?: string
  data?: unknown
  review?: VoiceReview
  handoff?: { route: string; operationId?: string }
  /** Human status-line labels for work this request did. */
  steps?: string[]
  /** Line the agent may speak; never contains approval codes. */
  narration?: string
  /** Office pages of records a completed write created or changed; shown, never spoken. */
  links?: { label: string | null; href: string }[]
  /** Why it did not complete (`failureReason` key); the agent says it in plain words. */
  reason?: string
  /** Client-only: the operation this execute result answered (picks the follow-up chip). */
  operationId?: string
}
export type VoiceAttachmentReceipt = {
  id: string
  fileId: string
  status: "uploaded"
}

/** Plain-language failure reason by HTTP status; the key names a localized `requestErrors` text. */
export function failureReason(status: number) {
  const reasons: Record<number, string> = {
    400: "invalid",
    401: "expired",
    403: "forbidden",
    404: "missing",
    409: "conflict",
    410: "conflict",
    413: "size",
    415: "format",
    422: "invalid",
    429: "busy",
    501: "unavailable",
  }
  return reasons[status] ?? "unverified"
}

/** Monitoring bucket: something Genesy may not cover, a refusal the user can fix, or a platform fault. */
export function failureCategory(reason: string) {
  if (reason === "unsupported") return "out_of_scope_candidate"
  if (reason === "unverified" || reason === "unavailable")
    return "platform_error"
  return "user_fixable"
}
