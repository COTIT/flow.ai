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
    const lots = find("batch expiry lot")
    expect(lots).toMatchObject({
      gaps: 2,
      workspaces: 2,
      languages: ["en", "sw"],
      rephrased: 1,
      usedUi: 1,
      route: "/lots",
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
    ["batch expiry lot", "discoverability"],
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
    expect(find("batch expiry lot")).toMatchObject({
      op: "lots_list",
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
      checkProposal("lot expiry batch", "lots_list", ["expiry", "batch"], {})
    ).toBeUndefined()
  })

  it("refuses words that do not bring it into the top three", () => {
    expect(checkProposal("lot expiry batch", "lots_list", [], {})).toMatch(
      /top three/
    )
  })

  it("refuses words that steer a read question to a write tool", () => {
    expect(
      checkProposal("lot expiry batch", "lots_create", ["expiry", "batch"], {})
    ).toMatch(/write/)
  })

  it("refuses words that lose probe replay hits", () => {
    const greedy =
      "order orders invoice invoices product products stock customer list create".split(
        " "
      )
    expect(
      checkProposal(
        "lot expiry batch",
        "lots_list",
        ["expiry", "batch", ...greedy],
        {}
      )
    ).toMatch(/replay/)
  })
})

describe("replayHits", () => {
  it("matches the replay regression floor with today's keywords", () => {
    expect(replayHits({})).toBeGreaterThanOrEqual(174)
  })
})

describe("planHeal", () => {
  it("proposes missing words for eligible discoverability clusters only", () => {
    expect(plan.proposals.map((p) => p.op).sort()).toEqual([
      "lots_list",
      "vat_groups_list",
    ])
    const lots = plan.proposals.find((p) => p.op === "lots_list")!
    expect([...lots.words].sort()).toEqual(["batch", "expiry"])
    expect(plan.keywords.lots_list!.en).toEqual(
      expect.arrayContaining(["batch", "expiry"])
    )
    expect(plan.keywords.lots_list!.nl).toEqual([])
    expect(plan.fixture).toContainEqual({
      intent: "lot expiry batch",
      want: "lots_list",
    })
    // Already found today: nothing to add, so no proposal.
    expect(plan.proposals.some((p) => p.op === "orders_top_customers")).toBe(
      false
    )
  })

  it("skips one workspace's few gaps unless the thresholds allow it", () => {
    expect(moneyPlan.skipped.map((s) => s.op)).toContain(
      "invoices_credit"
    )
    expect(loose.proposals.map((p) => p.op)).toContain(
      "invoices_credit"
    )
  })

  it("marks money and write operations for human review", () => {
    expect(loose.proposals.find((p) => p.op === "lots_list")!.review).toBe(
      false
    )
    expect(
      loose.proposals.find((p) => p.op === "invoices_credit")!
        .review
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
    expect(markdown).toContain("`lots_list` + batch, expiry")
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
