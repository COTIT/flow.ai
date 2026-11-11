/** An agent line with no tool call within this window is the answer, not "I'm checking…". */
export const SETTLE_MS = 1500
/** No narration or tool event for this long while the agent owes a reply: say it could not finish. */
export const STALL_MS = 45000

export type TurnEnd = "answered" | "stalled"

/** One user request until its answer: narration and tool calls keep it open; silence ends it. */
export function createTurn(onEnd: (kind: TurnEnd, steps: number) => void) {
  let open = false
  let tools = 0
  let steps = 0
  let started = false
  let settle: ReturnType<typeof setTimeout> | undefined
  let stall: ReturnType<typeof setTimeout> | undefined
  const clear = () => {
    clearTimeout(settle)
    clearTimeout(stall)
  }
  const finish = (kind: TurnEnd) => {
    clear()
    open = false
    onEnd(kind, steps)
  }
  const armStall = () => {
    clearTimeout(stall)
    if (open && !tools) stall = setTimeout(() => finish("stalled"), STALL_MS)
  }
  return {
    get open() {
      return open
    },
    begin() {
      clear()
      open = true
      started = false
      steps = 0
      armStall()
    },
    toolStart() {
      started = true
      tools++
      clear()
    },
    toolEnd() {
      tools = Math.max(0, tools - 1)
      steps++
      armStall()
    },
    agentLine() {
      if (!open) return
      started = true
      armStall()
      clearTimeout(settle)
      if (!tools) settle = setTimeout(() => finish("answered"), SETTLE_MS)
    },
    /** The agent is producing this answer; before that, a follow-up message merges into the request. */
    get working() {
      return open && (started || tools > 0)
    },
    /** Steps finished so far in the open turn. */
    get steps() {
      return open ? steps : 0
    },
    stop() {
      clear()
      open = false
      tools = 0
    },
  }
}

export type EndedBy =
  "user" | "ended_by_agent" | "time_limit" | "connection_lost"

/** Why the provider closed the conversation (SDK `onDisconnect` details). */
export function endedBy(details?: {
  reason?: string
  message?: string
  closeReason?: string
  context?: unknown
}): EndedBy {
  if (details?.reason === "user") return "user"
  if (details?.reason === "agent") return "ended_by_agent"
  const type = (details?.context as { type?: unknown } | undefined)?.type
  if (
    type === "max_duration_exceeded" ||
    /duration/i.test(`${details?.closeReason ?? ""} ${details?.message ?? ""}`)
  )
    return "time_limit"
  return "connection_lost"
}
