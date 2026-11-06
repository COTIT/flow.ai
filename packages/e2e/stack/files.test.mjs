import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { test } from "node:test"

import { FILES_IMAGE, ROOT } from "./stack.mjs"

test("the stack's file store is the image staging runs", () => {
  const overlay = readFileSync(join(ROOT, "deploy/staging/docker-compose.files.yml"), "utf8")
  assert.match(overlay, new RegExp(`image: ${FILES_IMAGE.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\n`))
})
