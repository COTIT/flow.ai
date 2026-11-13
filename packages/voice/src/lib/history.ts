import type { TranscriptRow } from "./helpers"

export type ConversationHistory = {
  id: string
  title: string
  state: "active" | "ended"
  revision: number
  coverage: "complete" | "provisional" | "partial"
  last_activity_at: string
  expires_at: string
}
export type HistoryMessage = {
  id: string
  source_id: string
  key: string
  ordinal: number
  role: "user" | "agent"
  text: string
  modality: "text" | "voice"
  status: "queued" | "sent_unconfirmed" | "complete"
  interrupted?: boolean
  provenance: string
}
export type HistoryPage = {
  messages: HistoryMessage[]
  revision: number
  cursor: string | null
  has_more: boolean
  through_sequence: string | number
  gaps: unknown[]
}
export type SessionHistory = {
  conversationId: string
  sourceId: string
  userId: string
  context: string
  revision: number
  coverage: ConversationHistory["coverage"]
}
export type HistoryEvent = Pick<
  HistoryMessage,
  "key" | "ordinal" | "role" | "text" | "modality" | "status" | "interrupted"
>

export const providerEventKey = (
  sourceId: string,
  role: "user" | "agent",
  eventId: string | number
) => `${sourceId}:${role}:${eventId}`

export function sessionHistoryOptions(
  enabled: boolean,
  continuing: boolean,
  context?: string
): { dynamicVariables: Record<string, string>; firstMessage?: string } {
  return enabled
    ? {
        dynamicVariables: { flow_history_context: context ?? "{}" },
        ...(continuing ? { firstMessage: "" } : {}),
      }
    : { dynamicVariables: { flow_history_context: "{}" } }
}

/** Archived text never enters live event handlers or carries approval capability. */
export function historyRows(messages: HistoryMessage[]): TranscriptRow[] {
  return messages.map(({ id, role, text, status }) => ({
    id: `history:${id}`,
    role,
    text,
    historical: true,
    ...(status !== "complete" ? { delivery: status } : {}),
  }))
}

type Locks = {
  request(
    name: string,
    options: { mode: "exclusive"; ifAvailable: true },
    use: (lock: object | null) => Promise<void>
  ): Promise<unknown>
}
/** No wait queue: another tab must release capture before a user explicitly retries. */
export function acquireMicrophone(
  locks: Locks | undefined,
  userId: string,
  workspaceId: string
): Promise<() => Promise<void>> {
  if (!locks) return Promise.reject(new Error("microphone_lock_unavailable"))
  return new Promise((resolve, reject) => {
    const operation = locks.request(
      `genesy.voice.microphone:${userId}:${workspaceId}`,
      { mode: "exclusive", ifAvailable: true },
      async (lock) => {
        if (!lock) {
          reject(new Error("microphone_in_use"))
          return
        }
        await new Promise<void>((release) =>
          resolve(() => {
            release()
            return operation.then(() => undefined)
          })
        )
      }
    )
    void operation.catch(reject)
  })
}
