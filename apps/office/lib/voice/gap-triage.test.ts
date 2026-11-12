import { describe, expect, it } from "vitest"

import { discoverTools } from "./catalog"
import rows from "./gap-triage.fixture.json"
import {
  checkProposal,
  classify,
  clusterGaps,
  type GapRow,
  planHeal,
  replayHits,
  report,
} from "./gap-triage"
import keywords from "./keywords.json"

const gaps = rows as GapRow[]
const triaged = clusterGaps(gaps).map(classify)
const find = (words: string, outcome?: string) =>
  triaged.find(
    (t) => t.words.join(" ") === words && (!outcome || t.outcome === outcome)
  )!
const shipped = JSON.stringify(keywords)
const thresholds = { minGaps: 2, minWorkspaces: 2, soloGaps: 5 }
// Each planHeal replays the probe fixture through discoverTools (~0.5 s), so each plan is built once.
const plan = planHeal(triaged, keywords, thresholds)
// Pin an eligible money write: broad payment words can correctly select an internal gateway.
const moneyCluster = {
  ...find("card payment refund"),
  kind: "discoverability" as const,
  domain: "invoices",
  op: "invoices_credit",
  found: false,
  words: ["credit", "invoice", "rebate"],
  intent: "credit invoice rebate",
  intents: ["credit invoice rebate"],
}
const moneyPlan = planHeal([moneyCluster], keywords, thresholds)
const loose = planHeal([...triaged, moneyCluster], keywords, {
  ...thresholds,
  minWorkspaces: 1,
  minGaps: 1,
})

describe("clusterGaps", () => {
  it("merges the same words in any order and counts distinct workspaces", () => {
    const bins = find("aisle bin slot")
    expect(bins).toMatchObject({
      gaps: 2,
      workspaces: 2,
      languages: ["en", "sw"],
      rephrased: 1,
      usedUi: 1,
      route: "/stock",
    })
    expect(find("cancel orders")).toMatchObject({
      gaps: 3,
      workspaces: 3,
      tool: "orders_cancel",
      httpStatus: 500,
      route: "/orders/[id]",
    })
  })

  it("puts the biggest clusters first", () => {
    expect(triaged[0]!.words).toEqual(["rate", "vat"])
    expect(triaged[0]!.gaps).toBe(5)
  })
})

describe("classify", () => {
  it.each([
    ["aisle bin slot", "discoverability"],
    ["rate vat", "discoverability"],
    ["customers orders top", "discoverability"],
    ["cancel orders", "bug"],
    ["create product", "bug"],
    ["delete invoice", "out_of_scope"],
    ["code pair shop till", "out_of_scope"],
    ["amount total", "missing"],
    ["unknown", "noise"],
    ["create orders", "noise"],
  ])("%s is %s", (words, kind) => {
    expect(find(words).kind).toBe(kind)
  })

  it("names the operation a discoverability gap needed, and whether search finds it today", () => {
    expect(find("aisle bin slot")).toMatchObject({
      op: "warehouse_bins_by_warehouse",
      found: false,
    })
    expect(find("customers orders top")).toMatchObject({
      op: "orders_top_customers",
      found: true,
    })
  })

  it("suspects the failing tool for bugs and offers the closest op for missing ones", () => {
    expect(find("cancel orders").op).toBe("orders_cancel")
    expect(find("amount total").op).toBeTruthy()
  })
})

describe("checkProposal", () => {
  it("accepts words that bring the operation into the top three", () => {
    expect(
      checkProposal(
        "aisle bin slot",
        "warehouse_bins_by_warehouse",
        ["aisle", "slot"],
        {}
      )
    ).toBeUndefined()
  })

  it("refuses words that do not bring it into the top three", () => {
    expect(
      checkProposal("aisle bin slot", "warehouse_bins_by_warehouse", [], {})
    ).toMatch(/top three/)
  })

  it("refuses words that steer a read question to a write tool", () => {
    expect(
      checkProposal(
        "aisle bin slot",
        "warehouse_bins_create",
        ["aisle", "slot"],
        {}
      )
    ).toMatch(/write/)
  })

  it("refuses words that lose probe replay hits", () => {
    const greedy =
      "order orders invoice invoices product products stock customer list create".split(
        " "
      )
    expect(
      checkProposal(
        "aisle bin slot",
        "warehouse_bins_by_warehouse",
        ["aisle", "slot", ...greedy],
        {}
      )
    ).toMatch(/replay/)
  })
})

describe("replayHits", () => {
  it("matches the replay regression floor with today's keywords", () => {
    expect(replayHits({})).toBeGreaterThanOrEqual(184)
  })
})

describe("planHeal", () => {
  it("proposes missing words for eligible discoverability clusters only", () => {
    // "vat rate" is found since the Office parity words: only the bin gap still needs words.
    expect(plan.proposals.map((p) => p.op).sort()).toEqual([
      "warehouse_bins_by_warehouse",
    ])
    const bins = plan.proposals.find(
      (p) => p.op === "warehouse_bins_by_warehouse"
    )!
    expect([...bins.words].sort()).toEqual(["aisle", "slot"])
    expect(plan.keywords.warehouse_bins_by_warehouse!.en).toEqual(
      expect.arrayContaining(["aisle", "slot"])
    )
    expect(plan.keywords.warehouse_bins_by_warehouse!.nl).toEqual([])
    expect(plan.fixture).toContainEqual({
      intent: "aisle bin slot",
      want: "warehouse_bins_by_warehouse",
    })
    // Already found today: nothing to add, so no proposal.
    expect(plan.proposals.some((p) => p.op === "orders_top_customers")).toBe(
      false
    )
  })

  it("skips one workspace's few gaps unless the thresholds allow it", () => {
    expect(moneyPlan.skipped.map((s) => s.op)).toContain("invoices_credit")
    expect(loose.proposals.map((p) => p.op)).toContain("invoices_credit")
  })

  it("marks money and write operations for human review", () => {
    expect(
      loose.proposals.find((p) => p.op === "warehouse_bins_by_warehouse")!
        .review
    ).toBe(false)
    expect(
      loose.proposals.find((p) => p.op === "invoices_credit")!.review
    ).toBe(true)
  })

  it("leaves the shipped keywords untouched", () => {
    expect(JSON.stringify(keywords)).toBe(shipped)
  })

  it("writes words that really make search find the operation", () => {
    const extra = Object.fromEntries(plan.proposals.map((p) => [p.op, p.words]))
    for (const { intent, want } of plan.fixture)
      expect(
        discoverTools(
          intent,
          ["workspace_owner", "farm:admin", "farm:ops"],
          extra
        )
          .slice(0, 3)
          .map((t) => t.id)
      ).toContain(want)
  })
})

describe("report", () => {
  const markdown = report(triaged, plan, { days: 7 })

  it("counts every class and lists what was proposed", () => {
    expect(markdown).toContain("# Voice gaps, last 7 days")
    expect(markdown).toMatch(
      /discoverability[^|]*\|\s*4 clusters\s*\|\s*9 gaps/
    )
    expect(markdown).toContain("`warehouse_bins_by_warehouse` + aisle, slot")
  })

  it("writes a paste-ready brief for every bug and missing capability", () => {
    expect(markdown.match(/^### Brief: /gm)).toHaveLength(3)
    expect(markdown).toContain("Suspected operation: `orders_cancel`")
    expect(markdown).toContain("3 gaps from 3 workspaces")
  })

  it("never shows workspace numbers", () => {
    expect(markdown).not.toMatch(/\bws\b/)
    for (const row of gaps) expect(markdown).not.toContain(`"ws": ${row.ws}`)
  })
})
