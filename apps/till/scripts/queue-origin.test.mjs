import assert from "node:assert/strict"
import { test } from "node:test"

import { migrateQueue, whereFrom } from "../lib/queue-origin.ts"

const here = { workspace: "w1", registerId: "reg-1", tillUserId: "tu-1" }
const item = (o) => ({
  workspace: "w1",
  registerId: "reg-1",
  tillUserId: "tu-1",
  ...o,
})

test("a sale is sent only by its own register and till user", () => {
  assert.equal(whereFrom(item(), here), "here")
  assert.equal(whereFrom(item({ tillUserId: "tu-2" }), here), "otherUser")
  assert.equal(whereFrom(item({ registerId: "reg-2" }), here), "otherRegister")
  assert.equal(whereFrom(item({ workspace: "w2" }), here), "otherRegister")
  assert.equal(
    whereFrom(item({ registerId: null, tillUserId: null }), here),
    "unknown"
  )
})

test("a sale saved before the till kept its origin migrates to unknown origin", () => {
  const old = {
    workspace: "w1",
    body: { client_ref: "r1" },
    lines: [],
    status: "pending",
  }
  const { items } = migrateQueue({ items: [old] }, 0)
  assert.deepEqual(items[0], { ...old, registerId: null, tillUserId: null })
  assert.equal(whereFrom(items[0], here), "unknown")
})

test("migrating an empty or missing store keeps it empty", () => {
  assert.deepEqual(migrateQueue({ items: [] }, 0), { items: [] })
  assert.deepEqual(migrateQueue(undefined, 0), { items: [] })
})
