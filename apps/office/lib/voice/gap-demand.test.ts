import { describe, expect, it } from "vitest"

import {
  demand,
  demandReport,
  fromDemandRow,
  type DemandRow,
} from "./gap-demand"
import type { Kind, Triaged } from "./gap-triage"

const row = (over: Partial<DemandRow> = {}): DemandRow => ({
  domain: "sales",
  outcome: "unsupported",
  words: "chart sales",
  intent: "sales chart",
  gaps: 7,
  workspaces: 3,
  languages: ["en", "sw"],
  rephrased: 2,
  used_ui: 4,
  route: "/sales",
  tool: "sales_summary",
  http_status: null,
  last_seen: "2026-11-09",
  ...over,
})

const triaged = (kind: Kind, over: Partial<DemandRow> = {}, op?: string) =>
  ({ ...fromDemandRow(row(over)), kind, op }) as Triaged & {
    lastSeen: string
  }

describe("fromDemandRow", () => {
  it("builds the clusterGaps key and shape from one aggregate row", () => {
    expect(fromDemandRow(row())).toEqual({
      key: "sales|unsupported|chart sales",
      domain: "sales",
      outcome: "unsupported",
      words: ["chart", "sales"],
      intent: "sales chart",
      intents: ["sales chart"],
      gaps: 7,
      workspaces: 3,
      languages: ["en", "sw"],
      rephrased: 2,
      usedUi: 4,
      route: "/sales",
      tool: "sales_summary",
      httpStatus: undefined,
      lastSeen: "2026-11-09",
    })
  })

  it("refuses per-row triage input: demand comes only from thresholded aggregates", () => {
    expect(() => fromDemandRow({ ...row(), ws: 1 } as DemandRow)).toThrow(
      /voice-gaps.sql -v demand=1/
    )
    expect(() =>
      fromDemandRow({ ...row(), workspace_id: "x" } as DemandRow)
    ).toThrow(/voice-gaps.sql -v demand=1/)
    expect(() => fromDemandRow(row({ workspaces: 2 }))).toThrow(
      /at least 3 workspaces/
    )
  })
})

describe("demand", () => {
  it("maps each triage class to one remedy and drops noise", () => {
    const items = demand(
      [
        triaged("noise", { words: "unknown", intent: "unknown" }),
        triaged(
          "bug",
          { words: "create order", intent: "create order", outcome: "failed" },
          "orders_create"
        ),
        triaged("out_of_scope", {
          words: "delete workspace",
          intent: "delete workspace",
          outcome: "refused",
        }),
        triaged(
          "discoverability",
          { words: "lot stock", intent: "lot stock" },
          "lots_stock"
        ),
        triaged("missing", { words: "loan rate", intent: "loan rate" }),
        triaged("missing", { words: "chart sales" }, "sales_summary"),
        triaged(
          "discoverability",
          { words: "orders table", intent: "orders table" },
          "orders_list"
        ),
      ],
      []
    )
    expect(
      items.map((i) => [i.words.join(" "), i.remedy, i.target ?? null])
    ).toEqual([
      ["create order", "fix", null],
      ["delete workspace", "clarification", null],
      ["lot stock", "discovery", null],
      ["loan rate", "capability", null],
      ["chart sales", "component", "new"],
      ["orders table", "property", "records"],
    ])
    const chart = items.find((i) => i.remedy === "component")!
    expect(chart).toMatchObject({
      presentation: "chart",
      op: "sales_summary",
      status: "open",
    })
  })

  it("marks resolved clusters, and recurring ones seen after the fix", () => {
    const key = "sales|unsupported|chart sales"
    const fix = {
      key,
      remedy: "component" as const,
      ref: "COTIT/genesy#99",
      at: "2026-11-01",
    }
    const [before] = demand(
      [triaged("missing", { last_seen: "2026-11-01" })],
      [fix]
    )
    const [after] = demand(
      [triaged("missing", { last_seen: "2026-11-09" })],
      [fix]
    )
    expect([before!.status, after!.status]).toEqual(["resolved", "recurring"])
    expect(after!.resolution).toEqual(fix)
  })

  it("never carries a workspace number or id", () => {
    const [item] = demand([triaged("missing")], [])
    expect(Object.keys(item!).sort()).toEqual([
      "domain",
      "gaps",
      "intent",
      "key",
      "languages",
      "lastSeen",
      "op",
      "outcome",
      "presentation",
      "remedy",
      "rephrased",
      "route",
      "status",
      "target",
      "usedUi",
      "words",
      "workspaces",
    ])
  })
})

describe("demandReport", () => {
  it("lists counts, remedy and status per cluster", () => {
    const md = demandReport(demand([triaged("missing")], []), {
      days: 7,
      minWorkspaces: 3,
    })
    expect(md).toContain(
      "# Component demand, last 7 days (clusters with at least 3 workspaces)"
    )
    expect(md).toContain(
      "| component new | sales | chart sales | 7 | 3 | en, sw | 2 | 4 /sales | open |"
    )
  })

  it("says so when nothing passed the threshold", () => {
    expect(demandReport([], { days: 7, minWorkspaces: 3 })).toContain(
      "No cluster reached 3 workspaces."
    )
  })
})
