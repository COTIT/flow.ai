import type { Cluster, Triaged } from "./gap-triage"

/** Developer-only component demand: k-anonymous gap clusters → structured remedies for review (Phase 6 input). */

/** Closed, reviewed presentation nouns `redactIntent` may keep; pinned by gaps.test.ts. */
export const PRESENTATION_WORDS: ReadonlySet<string> = new Set([
  "banner",
  "button",
  "calendar",
  "card",
  "chart",
  "color",
  "colour",
  "dashboard",
  "form",
  "gallery",
  "graph",
  "grid",
  "hero",
  "image",
  "layout",
  "list",
  "map",
  "menu",
  "page",
  "photo",
  "report",
  "section",
  "summary",
  "table",
  "timeline",
  "trend",
  "video",
  "view",
])

/** Presentation word → existing component that could grow a property (voice result views, storefront sections). */
const EXISTING: Record<string, string> = {
  table: "records",
  list: "records",
  timeline: "trail",
  hero: "hero",
  banner: "hero",
  grid: "product_grid",
  gallery: "product_grid",
}

/** Migration `assistant_gap_demand` is authoritative for k; this only refuses unthresholded input. */
const MIN_WORKSPACES = 3

/** One row of `voice-gaps.sql -v demand=1` (SQL `assistant_gap_demand`). */
export type DemandRow = {
  domain: string
  outcome: Cluster["outcome"]
  words: string
  intent: string
  gaps: number
  workspaces: number
  languages: string[]
  rephrased: number
  used_ui: number
  route: string | null
  tool: string | null
  http_status: number | null
  last_seen: string
}

export function fromDemandRow(row: DemandRow): Cluster & { lastSeen: string } {
  if ("ws" in row || "workspace_id" in row)
    throw new Error(
      "per-row input refused: use voice-gaps.sql -v demand=1 aggregates"
    )
  if (row.workspaces < MIN_WORKSPACES)
    throw new Error(`clusters need at least ${MIN_WORKSPACES} workspaces`)
  return {
    key: `${row.domain}|${row.outcome}|${row.words}`,
    domain: row.domain,
    outcome: row.outcome,
    words: row.words.split(" "),
    intent: row.intent,
    intents: [row.intent],
    gaps: row.gaps,
    workspaces: row.workspaces,
    languages: row.languages,
    rephrased: row.rephrased,
    usedUi: row.used_ui,
    route: row.route ?? undefined,
    tool: row.tool ?? undefined,
    httpStatus: row.http_status ?? undefined,
    lastSeen: row.last_seen,
  }
}

export type Remedy =
  | "component"
  | "property"
  | "discovery"
  | "capability"
  | "fix"
  | "clarification"
/** One entry of gap-demand.resolved.json: a shipped fix for a cluster key. */
export type Resolution = {
  key: string
  remedy: Remedy
  ref: string
  at: string
}
export type DemandItem = {
  key: string
  remedy: Remedy
  /** Existing component id for `property`, `new` for `component`. */
  target?: string
  presentation?: string
  /** Data source, suspected or closest operation. */
  op?: string
  domain: string
  outcome: Cluster["outcome"]
  words: string[]
  intent: string
  gaps: number
  workspaces: number
  languages: string[]
  rephrased: number
  usedUi: number
  route?: string
  lastSeen: string
  status: "open" | "resolved" | "recurring"
  resolution?: Resolution
}

function remedyOf(
  t: Triaged
): Pick<DemandItem, "remedy" | "target" | "presentation"> | undefined {
  if (t.kind === "noise") return undefined
  if (t.kind === "bug") return { remedy: "fix" }
  if (t.kind === "out_of_scope") return { remedy: "clarification" }
  const presentation = t.words.find((w) => PRESENTATION_WORDS.has(w))
  if (presentation)
    return EXISTING[presentation]
      ? { remedy: "property", target: EXISTING[presentation], presentation }
      : { remedy: "component", target: "new", presentation }
  return { remedy: t.kind === "discoverability" ? "discovery" : "capability" }
}

/** Rule-based remedy per cluster (first rule wins); a human still decides. Noise is dropped. */
export function demand(
  triaged: (Triaged & { lastSeen: string })[],
  resolved: Resolution[]
): DemandItem[] {
  return triaged.flatMap((t) => {
    const remedy = remedyOf(t)
    if (!remedy) return []
    const resolution = resolved.find((r) => r.key === t.key)
    return [
      {
        key: t.key,
        ...remedy,
        op: t.op,
        domain: t.domain,
        outcome: t.outcome,
        words: t.words,
        intent: t.intent,
        gaps: t.gaps,
        workspaces: t.workspaces,
        languages: t.languages,
        rephrased: t.rephrased,
        usedUi: t.usedUi,
        route: t.route,
        lastSeen: t.lastSeen,
        status: !resolution
          ? "open"
          : t.lastSeen > resolution.at
            ? "recurring"
            : "resolved",
        ...(resolution ? { resolution } : {}),
      },
    ]
  })
}

/** Weekly markdown of demand items; counts only. */
export function demandReport(
  items: DemandItem[],
  { days, minWorkspaces }: { days: number; minWorkspaces: number }
): string {
  const lines = [
    `# Component demand, last ${days} days (clusters with at least ${minWorkspaces} workspaces)`,
    "",
    ...(items.length
      ? [
          "| Remedy | Domain | Words | Gaps | Workspaces | Languages | Rephrased | Used UI | Status |",
          "|---|---|---|---|---|---|---|---|---|",
          ...items.map(
            (i) =>
              `| ${i.remedy}${i.target ? ` ${i.target}` : ""} | ${i.domain} | ${i.words.join(" ")} | ${i.gaps} | ${i.workspaces} | ${i.languages.join(", ")} | ${i.rephrased} | ${i.usedUi}${i.route ? ` ${i.route}` : ""} | ${i.status} |`
          ),
        ]
      : [`No cluster reached ${minWorkspaces} workspaces.`]),
  ]
  return `${lines.join("\n")}\n`
}
