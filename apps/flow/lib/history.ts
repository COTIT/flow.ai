import { sessionStore } from "@genesy/api-client/session"
import { voiceFetch } from "@genesy/voice/lib/fetch"
import { VoiceRequestError } from "@genesy/voice/lib/errors"

export type {
  ConversationHistory as HistoryConversation,
  HistoryMessage,
  HistoryPage as HistoryMessages,
} from "@genesy/voice/lib/history"

/** Same authenticated transport as the assistant; no conversation content is stored locally. */
export async function historyRequest<T>(
  body: Record<string, unknown>,
  signal?: AbortSignal
): Promise<T> {
  const { token, workspaceId } = sessionStore.getState()
  const { response, value } = await voiceFetch(
    "/api/voice",
    "history",
    {
      method: "POST",
      headers: {
        Authorization: `Bearer ${token}`,
        "Content-Type": "application/json",
      },
      body: JSON.stringify({ workspaceId, ...body }),
      cache: "no-store",
      signal,
    },
    () => sessionStore.getState().signOut("expired")
  )
  if (!response.ok) throw new VoiceRequestError("history", response.status)
  return value as T
}
