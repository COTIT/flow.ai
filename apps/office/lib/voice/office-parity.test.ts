import { describe, expect, it } from "vitest"

import { officeScreenOps, policy } from "../../scripts/office-parity-scan.mjs"
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
