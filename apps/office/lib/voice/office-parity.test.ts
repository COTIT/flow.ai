import { describe, expect, it } from "vitest"

import { officeScreenOps, policy } from "../../scripts/office-parity-scan.mjs"
import { coverage, coverageMarkdown } from "../../scripts/voice-coverage.mjs"
import { discoverTools } from "./catalog"
import manifest from "./office-parity.json"

// G1 + G5 of docs/genesy-flow/OFFICE_VOICE_COVERAGE.md: whatever an Office screen can do, voice can find,
// or the manifest says in plain words why it stays in Office.
type Entry = {
  ask?: Record<"en" | "nl" | "sw", string[]>
  officeOnlyBecause?: string
}
const entries = manifest as Record<string, Entry>
const scan = officeScreenOps()
const sayable = (op: string) =>
  ["execute", "secure_ui"].includes(policy[op]?.disposition ?? "")
const perms = ["workspace_owner", "farm:admin", "farm:ops"]
const top3 = (query: string) =>
  discoverTools(query, perms)
    .slice(0, 3)
    .map((t) => t.id)

describe("Office parity manifest", () => {
  it("lists every operation an Office screen calls", () => {
    const missing = [...scan.ops.keys()].filter((op) => !(op in entries))
    expect(missing.sort()).toEqual([])
  })

  it("lists only operations an Office screen still calls", () => {
    const stale = Object.keys(entries).filter((op) => !scan.ops.has(op))
    expect(stale.sort()).toEqual([])
  })

  it("matches every hand-built request to an API operation", () => {
    expect(scan.unmatched).toEqual([])
  })

  it.each(Object.entries(entries))(
    "%s: has ask phrases in en/nl/sw, or a reason it stays in Office",
    (op, entry) => {
      if (sayable(op)) {
        expect(entry.officeOnlyBecause, op).toBeUndefined()
        for (const lang of ["en", "nl", "sw"] as const)
          expect(entry.ask?.[lang]?.length, `${op} ${lang}`).toBeGreaterThan(0)
      } else {
        expect(entry.ask, op).toBeUndefined()
        expect(entry.officeOnlyBecause?.trim().length, op).toBeGreaterThan(20)
      }
    }
  )
})

const phrases = Object.entries(entries).flatMap(([op, entry]) =>
  Object.values(entry.ask ?? {})
    .flat()
    .map((phrase) => [phrase, op] as const)
)

describe("Office actions are findable by voice", () => {
  it.each(phrases)("%j finds %s", (phrase, op) => {
    expect(top3(phrase)).toContain(op)
  })
})

describe("coverage table for CI (G7)", () => {
  it("counts each area's actions by how voice reaches them", () => {
    const ops = new Map([
      ["orders_confirm", new Set(["app/(app)/orders/[id]/page.tsx"])],
      [
        "orders_list",
        new Set(["app/(app)/orders/page.tsx", "app/(app)/page.tsx"]),
      ],
      ["till_users_create", new Set(["app/(app)/till/users/page.tsx"])],
      ["invoices_gateway_refund", new Set(["components/invoice-refund.tsx"])],
      [
        "storefront_design_publish",
        new Set(["@genesy/api-client/storefront-review"]),
      ],
      ["auth_logout", new Set(["@genesy/api-client/auth"])],
    ])
    const fake = {
      orders_confirm: { disposition: "execute" },
      orders_list: { disposition: "execute" },
      till_users_create: { disposition: "secure_ui" },
      invoices_gateway_refund: { disposition: "internal" },
      storefront_design_publish: { disposition: "execute" },
      auth_logout: { disposition: "identity" },
    } as unknown as Parameters<typeof coverage>[1]
    expect(coverage(ops, fake)).toEqual([
      { area: "components", actions: 1, voice: 0, handoff: 0, officeOnly: 1 },
      { area: "orders", actions: 2, voice: 2, handoff: 0, officeOnly: 0 },
      { area: "overview", actions: 1, voice: 1, handoff: 0, officeOnly: 0 },
      { area: "settings", actions: 1, voice: 1, handoff: 0, officeOnly: 0 },
      {
        area: "sign-in and session",
        actions: 1,
        voice: 0,
        handoff: 0,
        officeOnly: 1,
      },
      { area: "till", actions: 1, voice: 0, handoff: 1, officeOnly: 0 },
    ])
  })

  it("writes a markdown table with totals", () => {
    const md = coverageMarkdown(
      [{ area: "orders", actions: 2, voice: 1, handoff: 1, officeOnly: 0 }],
      { execute: 413, withWords: 126, withPicture: 13 }
    )
    expect(md).toContain("| orders | 2 | 1 | 1 | 0 |")
    expect(md).toContain(
      "413 executable operations: 126 with trigger words, 13 drawn as a table or chart"
    )
  })
})
