import {
  discoverTools,
  discoveryText,
  matchesTerm,
  type ExtraWords,
  getOperation,
  isToolAllowed,
  operationManifest,
  queryTerms,
  type Tool,
} from "./catalog"
import replay from "./discovery-replay.fixture.json"

/** Developer-only gap triage: clusters ledger rows, classifies them and proposes trigger words. */

/** One `assistant_gaps` row from `voice-gaps.sql -v triage=1`; `ws` is a per-run number, never a workspace id. */
export type GapRow = {
  domain: string
  language: string
  intent: string
  outcome: "unsupported" | "refused" | "failed" | "cancelled" | "dropped"
  http_status: number | null
  tool: string | null
  follow_up: "rephrased" | "used_ui" | null
  follow_up_route: string | null
  ws: number
}

export type Cluster = {
  key: string
  domain: string
  outcome: GapRow["outcome"]
  words: string[]
  /** Most frequent redacted intent. */
  intent: string
  /** Distinct redacted intents, most frequent first. */
  intents: string[]
  gaps: number
  workspaces: number
  languages: string[]
  rephrased: number
  usedUi: number
  route?: string
  tool?: string
  httpStatus?: number
}

export type Kind =
  "discoverability" | "bug" | "missing" | "out_of_scope" | "noise"
export type Triaged = Cluster & {
  kind: Kind
  /** Needed (discoverability), suspected (bug), closest (missing) or excluded (out of scope) operation. */
  op?: string
  /** Discoverability only: search already ranks `op` in the top three today. */
  found?: boolean
}

export const KIND_LABEL: Record<Kind, string> = {
  discoverability: "a discoverability (op exists, not found)",
  bug: "b bug (op exists, failed)",
  missing: "c missing capability",
  out_of_scope: "d out of scope / correctly refused",
  noise: "e noise",
}

// Broadest staff view: workspace owner plus live farm roles, so farm operations count too.
const PERMS = ["workspace_owner", "farm:admin", "farm:ops"]
const searchable = (tool: Tool) =>
  ["execute", "secure_ui"].includes(tool.disposition)
const allowed = operationManifest.filter(
  (tool) => searchable(tool) && isToolAllowed(tool, PERMS)
)
const domains = [...new Set(operationManifest.map((tool) => tool.domain))]

function mode<T>(values: (T | null | undefined)[]): T | undefined {
  const counts = new Map<T, number>()
  for (const value of values)
    if (value != null) counts.set(value, (counts.get(value) ?? 0) + 1)
  return [...counts].sort((a, b) => b[1] - a[1])[0]?.[0]
}

export function clusterGaps(rows: GapRow[]): Cluster[] {
  const groups = new Map<string, GapRow[]>()
  for (const row of rows) {
    const words = [...new Set(row.intent.split(" "))].sort().join(" ")
    const key = `${row.domain}|${row.outcome}|${words}`
    groups.set(key, [...(groups.get(key) ?? []), row])
  }
  return [...groups]
    .map(([key, group]) => {
      const [first] = group as [GapRow, ...GapRow[]]
      const intents = new Map<string, number>()
      for (const { intent } of group)
        intents.set(intent, (intents.get(intent) ?? 0) + 1)
      return {
        key,
        domain: first.domain,
        outcome: first.outcome,
        words: [...new Set(first.intent.split(" "))].sort(),
        intent: [...intents].sort((a, b) => b[1] - a[1])[0]![0],
        intents: [...intents].sort((a, b) => b[1] - a[1]).map(([i]) => i),
        gaps: group.length,
        workspaces: new Set(group.map((row) => row.ws)).size,
        languages: [...new Set(group.map((row) => row.language))].sort(),
        rephrased: group.filter((row) => row.follow_up === "rephrased").length,
        usedUi: group.filter((row) => row.follow_up === "used_ui").length,
        route: mode(group.map((row) => row.follow_up_route)),
        tool: mode(group.map((row) => row.tool)),
        httpStatus: mode(group.map((row) => row.http_status)),
      }
    })
    .sort(
      (a, b) =>
        b.gaps - a.gaps ||
        b.workspaces - a.workspaces ||
        a.key.localeCompare(b.key)
    )
}

/** Domains a word points at: `lot` → lots, `vat` → vat_groups, `categories` → product_categories. */
const domainsFor = (raw: string) => {
  const word = raw.replace(/-/g, "_")
  return domains.filter(
    (d) =>
      d === word ||
      d === `${word}s` ||
      d.startsWith(`${word}_`) ||
      d.endsWith(`_${word}`) ||
      d.endsWith(`_${word}s`)
  )
}

const score = (tool: Tool, terms: string[], extra: ExtraWords = {}) => {
  const text = discoveryText(tool, extra)
  return terms.filter((term) => matchesTerm(text, term)).length
}

function best(tools: Tool[], terms: string[], domain = "") {
  return tools
    .map((tool) => ({ tool, score: score(tool, terms) }))
    .filter((item) => item.score > 0)
    .sort(
      (a, b) =>
        b.score - a.score ||
        Number(b.tool.domain === domain) - Number(a.tool.domain === domain) ||
        Number(b.tool.risk === "read") - Number(a.tool.risk === "read")
    )[0]
}

const top3 = (intent: string, extra: ExtraWords = {}) =>
  discoverTools(intent, PERMS, extra)
    .slice(0, 3)
    .map((tool) => tool.id)

/** Rule-based class; first matching rule wins. */
export function classify(cluster: Cluster): Triaged {
  const intent = cluster.intent
  if (
    intent === "unknown" ||
    cluster.outcome === "cancelled" ||
    cluster.outcome === "dropped"
  )
    return { ...cluster, kind: "noise" }
  if (cluster.outcome === "failed")
    return { ...cluster, kind: "bug", op: cluster.tool }
  if (cluster.outcome === "refused")
    // 400/422: the agent sent arguments the API rejects; other refusals are rules working.
    return {
      ...cluster,
      kind: [400, 422].includes(cluster.httpStatus ?? 0)
        ? "bug"
        : "out_of_scope",
      op: cluster.tool,
    }
  const terms = queryTerms(intent)
  const anywhere = best(operationManifest, terms)
  const reachable = best(allowed, terms)
  if (
    anywhere &&
    !searchable(anywhere.tool) &&
    anywhere.score >= 2 &&
    anywhere.score > (reachable?.score ?? 0)
  )
    return { ...cluster, kind: "out_of_scope", op: anywhere.tool.id }
  // Evidence of what the user wanted: the gap's domain, the page they opened next, the domains their words name.
  const evidence = new Set([
    ...(cluster.domain === "unknown" ? [] : [cluster.domain]),
    ...(cluster.route ?? "")
      .split("/")
      .filter((s) => s && s !== "[id]")
      .flatMap(domainsFor),
    ...cluster.words.flatMap(domainsFor),
  ])
  const needed = best(
    allowed.filter((tool) => evidence.has(tool.domain)),
    terms,
    cluster.domain
  )
  if (needed)
    return {
      ...cluster,
      kind: "discoverability",
      op: needed.tool.id,
      found: top3(intent).includes(needed.tool.id),
    }
  return { ...cluster, kind: "missing", op: reachable?.tool.id }
}

/** Probe replay queries whose needed operation discovery returns (the floor in discovery.test.ts). */
export function replayHits(extra: ExtraWords): number {
  return replay.filter(({ query, want }) => {
    const ids = discoverTools(query, ["workspace_owner"], extra).map(
      (t) => t.id
    )
    return want.some((id) => ids.includes(id))
  }).length
}

const merged = (extra: ExtraWords, op: string, words: string[]) => ({
  ...extra,
  [op]: [...(extra[op] ?? []), ...words],
})

/** Why adding `words` to `op` is unsafe, or undefined when it is fine. */
export function checkProposal(
  intent: string,
  op: string,
  words: string[],
  extra: ExtraWords
): string | undefined {
  const after = merged(extra, op, words)
  if (!top3(intent, after).includes(op))
    return `does not bring \`${op}\` into the top three`
  const [first] = discoverTools(intent, PERMS, extra)
  const [next] = discoverTools(intent, PERMS, after)
  if (
    first?.risk === "read" &&
    next &&
    next.id !== first.id &&
    next.risk !== "read"
  )
    return `would steer a read question to a write tool (\`${next.id}\` ahead of \`${first.id}\`)`
  const was = replayHits(extra)
  const now = replayHits(after)
  if (now < was) return `loses probe replay hits (${was} → ${now})`
  return undefined
}

export type Thresholds = {
  minGaps: number
  minWorkspaces: number
  soloGaps: number
}
type Keywords = Record<string, Record<string, string[]>>
export type Proposal = {
  op: string
  risk: string
  words: string[]
  intent: string
  gaps: number
  workspaces: number
  /** Write/high/secret risk or a money domain: a human merges it. */
  review: boolean
}
export type HealPlan = {
  proposals: Proposal[]
  rejected: { op: string; intent: string; reason: string }[]
  skipped: { op: string; intent: string; reason: string }[]
  keywords: Keywords
  fixture: { intent: string; want: string }[]
}

const MONEY = /passbook|payout|payment|invoice|cash|till|loan|voucher/

/** Trigger words for discoverability clusters, checked one by one on top of the earlier ones. */
export function planHeal(
  triaged: Triaged[],
  keywords: Keywords,
  { minGaps, minWorkspaces, soloGaps }: Thresholds
): HealPlan {
  const plan: HealPlan = {
    proposals: [],
    rejected: [],
    skipped: [],
    keywords: structuredClone(keywords),
    fixture: [],
  }
  let extra: ExtraWords = {}
  for (const cluster of triaged) {
    if (cluster.kind !== "discoverability" || cluster.found || !cluster.op)
      continue
    const tool = getOperation(cluster.op)!
    const intent = cluster.intent
    if (!(
      (cluster.gaps >= minGaps && cluster.workspaces >= minWorkspaces) ||
      cluster.gaps >= soloGaps
    )) {
      plan.skipped.push({ op: tool.id, intent, reason: "below threshold" })
      continue
    }
    const words = queryTerms(intent)
      .filter((word) => !matchesTerm(discoveryText(tool, extra), word))
      .sort()
    const reason = checkProposal(intent, tool.id, words, extra)
    if (reason) {
      plan.rejected.push({ op: tool.id, intent, reason })
      continue
    }
    extra = merged(extra, tool.id, words)
    plan.fixture.push({ intent, want: tool.id })
    plan.proposals.push({
      op: tool.id,
      risk: tool.risk,
      words,
      intent,
      gaps: cluster.gaps,
      workspaces: cluster.workspaces,
      review: tool.risk !== "read" || MONEY.test(tool.domain),
    })
  }
  for (const [op, words] of Object.entries(extra)) {
    const entry = (plan.keywords[op] ??= { en: [], nl: [], sw: [] })
    entry.en = [...new Set([...(entry.en ?? []), ...words])]
  }
  return plan
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`
const quote = (intent: string) => `"${intent}"`

/** Paste-ready issue for a bug or missing capability. */
export function brief(t: Triaged): string {
  const op = t.op && getOperation(t.op)
  return [
    `### Brief: ${t.kind === "bug" ? "voice bug" : "missing voice capability"} in \`${t.domain}\`: ${quote(t.intent)}`,
    "",
    `- Class: ${KIND_LABEL[t.kind]}`,
    `- ${plural(t.gaps, "gap")} from ${plural(t.workspaces, "workspace")}; languages ${t.languages.join(", ")}`,
    `- Outcome: ${t.outcome}${t.httpStatus ? `, HTTP ${t.httpStatus}` : ""}`,
    `- Example requests (redacted): ${t.intents.slice(0, 3).map(quote).join(", ")}`,
    op
      ? `- ${t.kind === "bug" ? "Suspected" : "Closest existing"} operation: \`${op.id}\` (${op.risk}) ${op.method} ${op.path}`
      : "- No operation identified",
    `- Afterwards: ${t.rephrased} rephrased, ${t.usedUi} opened a page${t.route ? ` (usually ${t.route})` : ""}`,
    t.kind === "bug"
      ? "- Done when: a failing API or Office test reproduces this outcome, the fix makes it pass, and the request completes by voice."
      : "- Done when: the capability exists in the API (with a workspace-isolation test), is in the voice catalog, and discovery finds it for these requests.",
    "",
  ].join("\n")
}

/** Weekly markdown: counts per class, top clusters, proposals, briefs. Counts only, never workspace numbers. */
export function report(
  triaged: Triaged[],
  plan: HealPlan,
  { days }: { days: number }
): string {
  const total = triaged.reduce((sum, t) => sum + t.gaps, 0)
  const kinds = Object.keys(KIND_LABEL) as Kind[]
  const lines = [
    `# Voice gaps, last ${days} days`,
    "",
    `${plural(total, "gap")} in ${plural(triaged.length, "cluster")}.`,
    "",
    "## By class",
    "",
    "| Class | Clusters | Gaps |",
    "|---|---|---|",
    ...kinds.map((kind) => {
      const of = triaged.filter((t) => t.kind === kind)
      return `| ${KIND_LABEL[kind]} | ${plural(of.length, "cluster")} | ${plural(
        of.reduce((sum, t) => sum + t.gaps, 0),
        "gap"
      )} |`
    }),
    "",
    "## Top clusters",
    "",
    "| Class | Domain | Outcome | Words | Gaps | Workspaces | Languages | Operation |",
    "|---|---|---|---|---|---|---|---|",
    ...triaged
      .slice(0, 10)
      .map(
        (t) =>
          `| ${t.kind} | ${t.domain} | ${t.outcome}${t.httpStatus ? ` ${t.httpStatus}` : ""} | ${t.words.join(" ")} | ${t.gaps} | ${t.workspaces} | ${t.languages.join(", ")} | ${t.op ? `\`${t.op}\`${t.found ? " (found today)" : ""}` : ""} |`
      ),
    "",
    "## Trigger words proposed",
    "",
    ...(plan.proposals.length
      ? plan.proposals.map(
          (p) =>
            `- \`${p.op}\` + ${p.words.join(", ")} for ${quote(p.intent)} (${plural(p.gaps, "gap")}, ${plural(p.workspaces, "workspace")})${p.review ? ", human review" : ""}`
        )
      : ["- none"]),
    ...plan.rejected.map(
      (r) => `- rejected \`${r.op}\` for ${quote(r.intent)}: ${r.reason}`
    ),
    ...plan.skipped.map(
      (s) => `- skipped \`${s.op}\` for ${quote(s.intent)}: ${s.reason}`
    ),
    "",
    "## Briefs",
    "",
    ...triaged
      .filter((t) => t.kind === "bug" || t.kind === "missing")
      .map(brief),
  ]
  return `${lines.join("\n").trimEnd()}\n`
}
