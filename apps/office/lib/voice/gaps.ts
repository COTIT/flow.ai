import "server-only"
import { assistantRequest } from "./bridge"
import { GAP_COOKIE, followUp, gapCookie } from "./gap-follow-up"
import { getOperation, operationManifest } from "./catalog"
import { PRESENTATION_WORDS } from "./gap-demand"
import type { VoiceContext } from "./server"

/** Developer-only gap ledger: what voice could not finish. Redacted summaries only, never transcripts. */
export type GapOutcome =
  "unsupported" | "refused" | "failed" | "cancelled" | "dropped"
export type GapSignal = {
  outcome: GapOutcome
  operationId?: string
  /** The agent's own English search phrase; preferred intent source. */
  query?: string
  /** Fallback intent source (e.g. user text); redacted like the query. */
  text?: string
  httpStatus?: number
}

const vocabulary = new Set([
  "for",
  "to",
  "from",
  "of",
  "with",
  "by",
  "and",
  "new",
  "all",
  ...PRESENTATION_WORDS,
  ...operationManifest.flatMap((tool) => [
    ...tool.id.split("_"),
    tool.domain,
    ...((tool.summary ?? "").toLowerCase().match(/[a-z]+/g) ?? []),
  ]),
])
const domains = new Set(operationManifest.map((tool) => tool.domain))

/** Keep only catalog words; fewer than two left means the request is counted as `unknown`. */
export function redactIntent(text = ""): string {
  const words = (
    text
      .toLowerCase()
      .replace(/\S+@\S+/g, " ")
      .match(/[a-z]+/g) ?? []
  )
    .filter((word) => vocabulary.has(word))
    .slice(0, 12)
  const intent = words.join(" ").slice(0, 120).trim()
  return words.length < 2 ? "unknown" : intent
}

function domainOf(intent: string, operationId?: string) {
  const known = operationId && getOperation(operationId)?.domain
  if (known) return known
  for (const word of intent.split(" "))
    for (const name of [word, `${word}s`]) if (domains.has(name)) return name
  return "unknown"
}

type Memory = {
  sessions: Map<
    string,
    { tools: string[]; at: number; gap?: string; gapAt?: number }
  >
}
const memoryKey = Symbol.for("genesy.office.voice.gaps.v1")
// shortcut: per-process memory; with several Office replicas tool history and rephrase links are per replica.
function memory(): Memory {
  const shared = globalThis as typeof globalThis & { [memoryKey]?: Memory }
  return (shared[memoryKey] ??= { sessions: new Map() })
}
function session(context: VoiceContext) {
  const sessions = memory().sessions
  const id = context.sessionId ?? ""
  const now = Date.now()
  for (const [key, value] of sessions)
    if (now - value.at > 300_000 || sessions.size > 500) sessions.delete(key)
  const entry = sessions.get(id) ?? { tools: [], at: now }
  entry.at = now
  sessions.set(id, entry)
  return entry
}

function withinOneSecond<T>(work: Promise<T>): Promise<T | undefined> {
  let timer: Parameters<typeof clearTimeout>[0]
  return Promise.race([
    work.catch(() => undefined),
    new Promise<undefined>((resolve) => (timer = setTimeout(resolve, 1000))),
  ]).finally(() => clearTimeout(timer))
}

/** Every tool call: remember the operation, and a call within 60 s of a gap marks it rephrased. */
export function noteToolCall(context: VoiceContext, operationId?: string) {
  if (!context.sessionId) return
  const entry = session(context)
  if (entry.gap && Date.now() - (entry.gapAt ?? 0) <= 60_000)
    void followUp(context.apiUrl, context.workspaceId, entry.gap, {
      signal: "rephrased",
    }).catch(() => undefined)
  entry.gap = undefined
  if (operationId && getOperation(operationId))
    entry.tools = [
      ...entry.tools.filter((t) => t !== operationId),
      operationId,
    ].slice(-5)
}

/** Record one gap; resolves to its id, or undefined when the ledger is slow or down. Never throws. */
export async function recordGap(
  context: VoiceContext,
  signal: GapSignal
): Promise<string | undefined> {
  const intent = redactIntent(signal.query || signal.text)
  const entry = context.sessionId ? session(context) : undefined
  if (entry && signal.operationId && getOperation(signal.operationId))
    entry.tools = [
      ...entry.tools.filter((t) => t !== signal.operationId),
      signal.operationId,
    ].slice(-5)
  const body = {
    domain: domainOf(intent, signal.operationId),
    language: ["en", "nl", "sw"].includes(context.language ?? "")
      ? context.language
      : "en",
    intent:
      intent === "unknown" &&
      signal.operationId &&
      getOperation(signal.operationId)
        ? redactIntent(signal.operationId.replace(/_/g, " "))
        : intent,
    tools: entry?.tools ?? [],
    outcome: signal.outcome,
    ...(signal.httpStatus &&
    signal.httpStatus >= 400 &&
    signal.httpStatus <= 599
      ? { http_status: signal.httpStatus }
      : {}),
  }
  const result = await withinOneSecond(
    assistantRequest<{ id: string }>(context, "/gaps", "POST", body, true)
  )
  if (result?.id && entry) {
    entry.gap = result.id
    entry.gapAt = Date.now()
  }
  return result?.id
}

/** What a voice route learned about the request before answering. */
export type GapScope = {
  context?: VoiceContext
  operationId?: string
  query?: string
  cancelled?: boolean
}

/** Record a gap when the route's answer carries a failure `reason` (or the user cancelled); success passes through. */
export async function trackGap(
  scope: GapScope,
  response: Response
): Promise<Response> {
  if (!scope.context) return response
  let body: { reason?: unknown; httpStatus?: unknown } = {}
  try {
    body = await response.clone().json()
  } catch {
    /* Non-JSON answers carry no failure reason. */
  }
  const reason = typeof body.reason === "string" ? body.reason : undefined
  if (!reason && !scope.cancelled) return response
  const httpStatus =
    typeof body.httpStatus === "number" ? body.httpStatus : response.status
  const outcome: GapOutcome = scope.cancelled
    ? "cancelled"
    : reason === "unsupported"
      ? "unsupported"
      : httpStatus >= 500 || reason === "unverified" || reason === "unavailable"
        ? "failed"
        : "refused"
  const id = await recordGap(scope.context, {
    outcome,
    operationId: scope.operationId,
    query: scope.query?.replace(/^unsupported:\s*/i, ""),
    httpStatus,
  })
  if (!id) return response
  const tracked = new Response(response.body, response)
  tracked.headers.append(
    "Set-Cookie",
    `${GAP_COOKIE}=${await gapCookie(scope.context.workspaceId, id)}; Path=/; Max-Age=300; HttpOnly; Secure; SameSite=Lax`
  )
  return tracked
}
