import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"

const scenarios = JSON.parse(readFileSync(new URL("./scenarios.json", import.meta.url), "utf8"))
const catalog = JSON.parse(
  readFileSync(new URL("../../../apps/office/lib/voice/catalog.generated.json", import.meta.url), "utf8")
)
const ids = new Set(catalog.map((op) => op.id))

const KIND_OUTCOMES = {
  read: ["answer", "clarify"],
  write: ["review", "clarify", "refuse"],
  "refuse-expected": ["refuse"],
  "out-of-scope": ["out_of_scope"],
}

test("the bank holds at least 90 scenarios with unique ids", () => {
  assert.ok(scenarios.length >= 90, `${scenarios.length} scenarios`)
  assert.equal(new Set(scenarios.map((s) => s.id)).size, scenarios.length)
})

test("every scenario is well formed", () => {
  for (const s of scenarios) {
    const where = `scenario ${s.id}`
    assert.match(s.id, /^[a-z0-9-]+$/, where)
    assert.ok(["morning", "orders", "products", "stock", "supply", "invoices", "till", "admin", "search", "hard"].includes(s.module), where)
    assert.ok(["en", "sw", "mixed"].includes(s.lang), where)
    assert.ok(["afm", "ruaha"].includes(s.as ?? "afm"), where)
    assert.ok(Array.isArray(s.turns) && s.turns.length >= 1 && s.turns.length <= 3, where)
    assert.ok(s.turns.every((t) => typeof t === "string" && t.length > 2), where)
    assert.ok(KIND_OUTCOMES[s.kind]?.includes(s.expect?.outcome), `${where}: ${s.kind} → ${s.expect?.outcome}`)
    if (s.expect.mention) new RegExp(s.expect.mention, "i")
  }
})

test("expected operations exist in the voice catalog; a scenario without one names its gap", () => {
  for (const s of scenarios) {
    for (const op of s.expect.ops ?? []) assert.ok(ids.has(op), `${s.id}: unknown op ${op}`)
    if (["answer", "review"].includes(s.expect.outcome) && !(s.expect.ops ?? []).length && !s.expect.noTool)
      assert.ok(typeof s.gap === "string" && s.gap.length > 5, `${s.id}: no ops and no gap`)
  }
})

test("the parent's six staging findings are regression cases", () => {
  assert.ok(scenarios.filter((s) => s.regression).length >= 6)
})
