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
}
export type VoiceAttachmentReceipt = {
  id: string
  fileId: string
  status: "uploaded"
}
