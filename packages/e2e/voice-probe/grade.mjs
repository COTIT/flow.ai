// Rule-based grader: one probe scenario + what the runner recorded → a class and UI defects.
// Pure and offline; the runner and the unit tests both call it.

export const CLASSES = ["works", "partial", "missed_tool", "missing_capability", "broken", "correctly_refused"]

const NARRATION =
  /^(ok(ay)?[,.!]?\s*)?((i'?m|i am) (now )?(checking|looking|pulling|fetching|getting|searching|gathering)|let me|i'?ll (check|look|pull|fetch|get|search|find)|one moment|just a (moment|sec)|give me a (moment|sec)|hold on|checking|looking (that|this|it) up|ninaangalia|naangalia|subiri|hebu|ngoja)\b/i

/** A "please wait" line that promises work instead of reporting a result. */
export const isNarration = (text) => text.trim().length < 240 && NARRATION.test(text.trim())

const CANT =
  /\b(can'?t|cannot|can not|unable|not able|don'?t have (access|a way|the ability|a tool)|not (possible|supported|available)|no (tool|way) to|isn'?t (possible|supported|available)|siwezi|haiwezekani|sina uwezo)\b/i
const QUESTION = /\?\s*$/m

const DETECTORS = [
  ["raw_tool_text", /`[^`]+`|\(a page, or|\b(page_size|workspace_id|operation_id|arguments_json|operationId)\b/],
  ["raw_uuid", /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/i],
  ["raw_json", /[{[]\s*"\w+"\s*:/],
  ["raw_http_code", /\b[45]\d\d\b\s*(unprocessable|error|forbidden|not found|bad request|internal|conflict|unauthori[sz]ed)|\b(status|http|error)\s*(code\s*)?[45]\d\d\b/i],
  // The probe browser runs in Asia/Manila on purpose: a Manila time on screen is browser time, not the account's.
  ["browser_time_zone", /\b(manila|philippine|PHT)\b|\bUTC\+0?8\b|\bGMT\+0?8\b/i],
  ["leaked_error", /\b(undefined|NaN|\[object Object\]|TypeError|stack trace)\b/],
]

/** Raw codes or tool internals in text a user can see. */
export function defectsIn(texts, operationIds) {
  const out = []
  for (const text of texts) {
    for (const [kind, re] of DETECTORS) if (re.test(text)) out.push({ kind, text: text.slice(0, 200) })
    const id = text.match(/\b[a-z]+(?:_[a-z]+)+\b/g)?.find((w) => operationIds.has(w))
    if (id) out.push({ kind: "raw_operation_id", text: text.slice(0, 200) })
  }
  return out
}

/**
 * @param scenario  entry of scenarios.json
 * @param rec       { turns[{finished}], messages[{role,text}], toolCalls[{command,operationId,http,status}],
 *                    reviews, handoffs, approveAttempts, alerts[], thinking[] }
 * @param catalog   catalog.generated.json entries ({ id, disposition })
 */
export function grade(scenario, rec, catalog) {
  const reachable = new Set(catalog.filter((o) => ["execute", "secure_ui"].includes(o.disposition)).map((o) => o.id))
  const ids = new Set(catalog.map((o) => o.id))
  const agent = rec.messages.filter((m) => m.role === "agent").map((m) => m.text)
  const last = agent.at(-1) ?? ""
  const shown = [...agent, ...rec.messages.filter((m) => m.role === "action").map((m) => m.text), ...rec.thinking]
  const defects = defectsIn(shown, ids)
  const called = new Set(rec.toolCalls.filter((c) => c.command === "execute").map((c) => c.operationId))
  const expected = scenario.expect.ops ?? []
  const hit = expected.filter((op) => called.has(op))
  const proposed = rec.reviews + rec.handoffs > 0
  const cant = CANT.test(last)
  const asked = QUESTION.test(last)
  const out = (cls, why) => ({ class: cls, why, defects, called: [...called] })

  if (rec.approveAttempts > 0) return out("broken", `probe attempted ${rec.approveAttempts} approve/submit request(s); blocked`)
  const stuck = rec.turns.find((t) => t.finished !== "answered")
  if (stuck?.finished === "narration_only") return out("broken", `narrated "${last.slice(0, 80)}" but gave no final answer`)
  if (stuck) return out("broken", `turn "${stuck.user.slice(0, 60)}" ended: ${stuck.finished}`)
  if (!agent.length) return out("broken", "no agent message at all")
  if (rec.alerts.length) return out("broken", `error shown: ${rec.alerts[0]}`)
  const failed = rec.toolCalls.find((c) => c.http >= 500)
  if (failed) return out("broken", `tool ${failed.operationId ?? failed.command} returned HTTP ${failed.http}`)

  const { outcome, mention, noTool } = scenario.expect
  if (outcome === "refuse" || outcome === "out_of_scope")
    return proposed ? out("partial", "proposed a write it should have refused") : out("correctly_refused", "declined, nothing proposed")

  const unable = () =>
    expected.some((op) => reachable.has(op))
      ? out("missed_tool", `said it can't; reachable: ${expected.filter((op) => reachable.has(op)).join(", ")}`)
      : out("missing_capability", scenario.gap ?? `no reachable op among ${expected.join(", ") || "none"}`)

  if (outcome === "clarify") {
    if (proposed) return out("partial", "acted without asking first")
    if (asked) return out("works", "asked a clarifying question")
    return cant ? unable() : out("partial", "neither asked nor acted")
  }
  if (outcome === "review") {
    if (proposed) return out("works", `review shown${hit.length ? ` via ${hit.join(", ")}` : ""}`)
    if (cant) return unable()
    if (asked) return out("partial", "asked a question instead of proposing the write")
    return out("partial", "no review card for a write request")
  }
  // outcome === "answer"
  if (cant) return noTool ? out("partial", "declined a how-to question") : unable()
  if (mention && !new RegExp(mention, "i").test(agent.join("\n"))) return out("partial", `answer lacks /${mention}/`)
  if (noTool) return out("works", "answered from knowledge")
  if (hit.length) return out("works", `answered via ${hit.join(", ")}`)
  if (!called.size) return out("partial", "answered without calling any tool")
  return out("partial", `answered via ${[...called].join(", ")}; expected one of ${expected.join(", ")}`)
}
