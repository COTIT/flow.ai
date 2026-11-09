import assert from "node:assert/strict"
import { existsSync, readFileSync } from "node:fs"
import { test } from "node:test"

const load = (name) => JSON.parse(readFileSync(new URL(name, import.meta.url), "utf8"))
const catalog = load("../../../apps/office/lib/voice/catalog.generated.json")
const ids = new Set(catalog.map((op) => op.id))

const KIND_OUTCOMES = {
  read: ["answer", "clarify"],
  write: ["review", "clarify", "refuse"],
  "refuse-expected": ["refuse"],
  "out-of-scope": ["out_of_scope"],
}
const BANKS = [
  { file: "scenarios.json", min: 90, maxTurns: 3 },
  { file: "adversarial.json", min: 20, maxTurns: 12 },
]
const all = BANKS.flatMap((b) => load(b.file))

test("ids are unique across both banks", () => {
  assert.equal(new Set(all.map((s) => s.id)).size, all.length)
})

for (const { file, min, maxTurns } of BANKS) {
  const scenarios = load(file)

  test(`${file}: holds at least ${min} well-formed scenarios`, () => {
    assert.ok(scenarios.length >= min, `${scenarios.length} scenarios`)
    for (const s of scenarios) {
      const where = `scenario ${s.id}`
      assert.match(s.id, /^[a-z0-9-]+$/, where)
      assert.ok(["morning", "orders", "products", "stock", "supply", "invoices", "till", "admin", "search", "hard"].includes(s.module), where)
      assert.ok(["en", "sw", "nl", "mixed"].includes(s.lang), where)
      assert.ok(["afm", "ruaha"].includes(s.as ?? "afm"), where)
      assert.ok(Array.isArray(s.turns) && s.turns.length >= 1 && s.turns.length <= maxTurns, where)
      assert.ok(s.turns.every((t) => typeof t === "string" && t.length > 2), where)
      assert.ok(KIND_OUTCOMES[s.kind]?.includes(s.expect?.outcome), `${where}: ${s.kind} → ${s.expect?.outcome}`)
      if (s.expect.mention) new RegExp(s.expect.mention, "i")
      for (const f of s.attach ?? []) assert.ok(existsSync(new URL(f, import.meta.url)), `${where}: missing ${f}`)
    }
  })

  test(`${file}: expected operations exist in the voice catalog; a scenario without one names its gap`, () => {
    for (const s of scenarios) {
      for (const op of s.expect.ops ?? []) assert.ok(ids.has(op), `${s.id}: unknown op ${op}`)
      if (["answer", "review"].includes(s.expect.outcome) && !(s.expect.ops ?? []).length && !s.expect.noTool)
        assert.ok(typeof s.gap === "string" && s.gap.length > 5, `${s.id}: no ops and no gap`)
    }
  })
}

test("the parent's six staging findings are regression cases", () => {
  assert.ok(load("scenarios.json").filter((s) => s.regression).length >= 6)
})
