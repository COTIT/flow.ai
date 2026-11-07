import type { VoiceOutcome } from "@/lib/voice/client-contract"
import type { BatchProgress } from "./continuity"

export const RECOVERY_KEY = "genesy.voice.recovery"
export type RecoveryPointer = { sessionId: string; workspaceId: string }
type TabStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">
const opaque = (value: unknown): value is string =>
  typeof value === "string" && /^[a-zA-Z0-9_-]{1,100}$/.test(value)

/** Browser remembers identity pointers only; every read still needs server authorization. */
export function readRecovery(
  storage: TabStorage,
  workspaceId: string
): RecoveryPointer | null {
  try {
    const value = JSON.parse(storage.getItem(RECOVERY_KEY) ?? "null")
    if (
      !value ||
      typeof value !== "object" ||
      Array.isArray(value) ||
      Object.keys(value).sort().join(",") !== "sessionId,workspaceId" ||
      !opaque(value.sessionId) ||
      !opaque(value.workspaceId) ||
      value.workspaceId !== workspaceId
    )
      return null
    return { sessionId: value.sessionId, workspaceId: value.workspaceId }
  } catch {
    return null
  }
}
export function saveRecovery(
  storage: TabStorage,
  pointer: RecoveryPointer | null
) {
  try {
    if (!pointer) storage.removeItem(RECOVERY_KEY)
    else if (opaque(pointer.sessionId) && opaque(pointer.workspaceId))
      storage.setItem(
        RECOVERY_KEY,
        JSON.stringify({
          sessionId: pointer.sessionId,
          workspaceId: pointer.workspaceId,
        })
      )
  } catch {
    /* Storage denial must not stop a live conversation. */
  }
}
export const sessionLanguage = (locale: string): "en" | "nl" | "sw" =>
  locale === "nl" || locale === "sw" ? locale : "en"

/** Native client-tool result resumes the agent without fabricating a user message. */
export function createOutcomeWait(fallback: VoiceOutcome) {
  let settled = false
  let resolve!: (value: VoiceOutcome) => void
  const result = new Promise<VoiceOutcome>((done) => {
    resolve = done
  })
  const timer = setTimeout(() => finish(fallback), 80000)
  function finish(value: VoiceOutcome) {
    if (settled) return
    settled = true
    clearTimeout(timer)
    resolve(value)
  }
  return { result, finish }
}

export type RecoveryTask = VoiceOutcome & {
  data?: {
    actionId?: string
    batchId?: string
    kind?: "batch"
    state?: string
    steps?: BatchProgress["steps"]
    summary?: string
    operation?: string
    evidence?: unknown
    recoverable?: boolean
  }
}
