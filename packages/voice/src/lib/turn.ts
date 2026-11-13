/** An agent line with no tool call within this window is the answer, not "I'm checking…". */
export const SETTLE_MS = 1500
/** No narration or tool event for this long while the agent owes a reply: say it could not finish. */
export const STALL_MS = 45000

/** "quiet": the agent went silent after a tool call it made on its own; nobody is owed an answer. */
export type TurnEnd = "answered" | "stalled" | "quiet"

/** One user request until its answer: narration and tool calls keep it open; silence ends it. */
export function createTurn(onEnd: (kind: TurnEnd, steps: number) => void) {
  let open = false
  let tools = 0
  let steps = 0
  let started = false
  let answer = false
  let epoch = 0
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
    // Outside a user turn (the agent acting on its own) the same silence ends quietly (P7-3).
    if (!tools)
      stall = setTimeout(() => finish(open ? "stalled" : "quiet"), STALL_MS)
  }
  return {
    get open() {
      return open
    },
    begin() {
      clear()
      epoch++
      open = true
      tools = 0
      started = false
      answer = false
      steps = 0
      armStall()
    },
    toolStart() {
      started = true
      answer = false
      tools++
      clear()
      return epoch
    },
    toolEnd(token = epoch) {
      if (token !== epoch) return
      tools = Math.max(0, tools - 1)
      steps++
      armStall()
      if (!tools && open && answer) settle = setTimeout(() => finish("answered"), SETTLE_MS)
    },
    agentLine() {
      if (!open) return clearTimeout(stall)
      started = true
      answer = true
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
      epoch++
      open = false
      tools = 0
    },
  }
}

/** Typed requests and their provider echo share one identity; replayed speech never restarts it. */
export function createRequests(newId: () => string) {
  let current: string | null = null
  const typed: { text: string; id: string }[] = []
  const heard = new Map<string, string>()
  const words = (text: string) => text.trim().replace(/\s+/g, " ")
  return {
    get current() { return current },
    typed(text: string) {
      current = newId()
      typed.push({ text: words(text), id: current })
      if (typed.length > 200) typed.shift()
      return current
    },
    heard(text: string, key?: string) {
      const replay = key ? heard.get(key) : undefined
      if (replay) return { id: replay, fresh: false }
      const index = typed.findIndex((entry) => entry.text === words(text))
      const echo = index < 0 ? null : typed.splice(index, 1)[0]!.id
      const id = echo ?? newId()
      if (!echo) current = id
      if (key) {
        heard.set(key, id)
        if (heard.size > 200) heard.delete(heard.keys().next().value!)
      }
      return { id, fresh: !echo }
    },
    finish() {
      typed.splice(0)
    },
    clear() {
      current = null
      typed.splice(0)
      heard.clear()
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
