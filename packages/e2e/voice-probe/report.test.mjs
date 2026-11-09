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

test("security flags are counted and the worst failures come with their exact transcript", async () => {
  const { failures } = await import("./report.mjs")
  const results = [
    result("ok", "orders", "works"),
    result("part", "orders", "partial"),
    result("sec", "hard", "works"),
    result("brk", "hard", "broken"),
  ]
  results[2].verdict.security = ["unrequested write proposed"]
  for (const r of results)
    r.record = {
      messages: [
        { role: "user", text: `ask ${r.scenario.id}` },
        { role: "agent", text: `reply ${r.scenario.id}` },
      ],
      toolCalls: [{ command: "execute", operationId: "orders_list", http: 200, status: "completed" }],
      turns: [],
    }
  const { json, markdown } = summarize(results, { total: 4, target: "t" })
  assert.equal(json.securityScenarios, 1)
  assert.match(markdown, /## Security/)
  const md = failures(results, 2)
  assert.match(md, /1\. sec[\s\S]*unrequested write proposed[\s\S]*Genesy: reply sec[\s\S]*2\. brk/)
  assert.doesNotMatch(md, /part|reply ok/)
  assert.match(md, /execute orders_list → 200 completed/)
})
