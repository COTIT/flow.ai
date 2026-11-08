import assert from "node:assert/strict"
import { test } from "node:test"

import { discover, inventory, verify } from "../verify-shards.mjs"

const spec = {
  file: "tests/a.spec.ts",
  line: 3,
  column: 1,
  title: "a test",
  tests: [{ projectName: "desktop" }, { projectName: "mobile" }],
}
const report = {
  suites: [
    { title: "tests/a.spec.ts", suites: [{ title: "group", specs: [spec] }] },
  ],
}

test("shard coverage preserves projects and accepts reordered complete partitions", () => {
  const all = inventory(report)
  assert.equal(all.length, 2)
  assert.notEqual(all[0], all[1])
  verify(all, [[all[1]], [all[0]]])
})

test("missing, duplicated or empty selections cannot pass coverage validation", () => {
  for (const shards of [[["a"]], [["a"], ["a", "b"]], [["a", "b"], []]])
    assert.throws(() => verify(["a", "b"], shards))
  assert.throws(() => verify([], []))
  assert.throws(() => inventory({ errors: ["bad config"], suites: [] }))
})

test("discovery forwards exact shard and propagates process or reporter errors", () => {
  const run = (cmd, args) => {
    assert.equal(cmd, "pnpm")
    assert.ok(args.includes("--list"))
    assert.ok(args.includes("--shard=2/8"))
    return { status: 0, stdout: JSON.stringify(report) }
  }
  assert.deepEqual(
    discover("playwright.config.ts", "2/8", run),
    inventory(report)
  )
  assert.throws(
    () => discover("x", "", () => ({ status: 1, stderr: "bad config" })),
    /bad config/
  )
  assert.throws(() =>
    discover("x", "", () => ({ status: 0, stdout: "invalid" }))
  )
})
