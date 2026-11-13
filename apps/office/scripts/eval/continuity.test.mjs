import assert from "node:assert/strict"
import { test } from "vitest"
import { continuityFixture, verifyContinuityReply } from "./continuity.mjs"

test("synthetic continuity fixtures separate bootstrap recall from older exact retrieval", () => {
  const recent = continuityFixture(false),
    older = continuityFixture(true)
  assert(recent.context.includes("charcoal"))
  assert(!older.context.includes("charcoal"))
  assert(!older.question.includes("138"))
  assert(older.archive.some((row) => row.text.includes("charcoal")))
  assert.equal(JSON.parse(older.context).messages.length, 20)
})

test("real-provider gate rejects stale facts, missing exact values and claimed approval", () => {
  verifyContinuityReply(
    '{"color":"charcoal","budget":138,"publishing_approved":false}'
  )
  for (const reply of [
    '{"color":"blue","budget":84,"publishing_approved":false}',
    '{"color":"charcoal","budget":138,"publishing_approved":true}',
    "I remember everything.",
  ])
    assert.throws(() => verifyContinuityReply(reply))
})
