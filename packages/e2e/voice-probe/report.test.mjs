import assert from "node:assert/strict"
import { test } from "node:test"

import { summarize } from "./report.mjs"

const result = (id, module, cls, extra = {}) => ({
  scenario: { id, module, kind: "read", turns: ["q"], expect: { outcome: "answer" }, ...extra },
  verdict: { class: cls, why: `${cls} because`, defects: [], called: ["orders_list"] },
})

test("summarises classes per module, defects, regressions and next steps", () => {
  const results = [
    result("a", "orders", "works"),
    result("b", "orders", "broken", { regression: "staging: no final answer" }),
    result("c", "till", "missing_capability", { gap: "till device flow" }),
    result("d", "stock", "missed_tool"),
  ]
  results[0].verdict.defects = [{ kind: "raw_tool_text", text: "Checking: `limit`" }]
  const { json, markdown } = summarize(results, { total: 93, target: "local stack" })
  assert.equal(json.counts.works, 1)
  assert.equal(json.counts.broken, 1)
  assert.equal(json.defectScenarios, 1)
  assert.match(markdown, /\| orders \| 1 \| 0 \| 0 \| 0 \| 1 \| 0 \| 1 \|/)
  assert.match(markdown, /## Regression cases/)
  assert.match(markdown, /staging: no final answer/)
  assert.match(markdown, /raw_tool_text/)
  assert.match(markdown, /New tool needed[\s\S]*c/)
  assert.match(markdown, /Wording or discovery fix[\s\S]*d/)
  assert.match(markdown, /4 of 93 scenarios ran/)
})
