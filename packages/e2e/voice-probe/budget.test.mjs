import assert from "node:assert/strict"
import { mkdtempSync, readFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { test } from "node:test"

import { takeConversation } from "./budget.mjs"

test("counts every conversation in a ledger and refuses once the budget is spent", () => {
  const ledger = join(mkdtempSync(join(tmpdir(), "probe-")), "conversations.log")
  assert.equal(takeConversation(ledger, 2, "a"), true)
  assert.equal(takeConversation(ledger, 2, "b"), true)
  assert.equal(takeConversation(ledger, 2, "c"), false)
  const lines = readFileSync(ledger, "utf8").trim().split("\n")
  assert.equal(lines.length, 2)
  assert.match(lines[1], /\tb$/)
})

test("no budget set means no limit, but every conversation is still logged", () => {
  const ledger = join(mkdtempSync(join(tmpdir(), "probe-")), "conversations.log")
  assert.equal(takeConversation(ledger, undefined, "a"), true)
  assert.equal(readFileSync(ledger, "utf8").trim().split("\n").length, 1)
})
