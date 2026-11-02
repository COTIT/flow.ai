import assert from "node:assert/strict"
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { test } from "node:test"

import { findMotion, KILL_RULE } from "./no-motion.mjs"

/** A fake `.next/static` folder with these files in it. */
function bundle(files) {
  const dir = mkdtempSync(join(tmpdir(), "till-no-motion-"))
  for (const [name, text] of Object.entries(files)) {
    mkdirSync(join(dir, name, ".."), { recursive: true })
    writeFileSync(join(dir, name), text)
  }
  return dir
}

const cleanCss = `@keyframes pulse{50%{opacity:.5}}.animate-pulse{animation:var(--animate-pulse)}${KILL_RULE}`

test("a clean bundle passes, unused-by-rule @keyframes included", () => {
  const dir = bundle({
    "chunks/app.js": 'console.log("till")',
    "chunks/app.css": cleanCss,
  })
  assert.deepEqual(findMotion(dir), [])
})

for (const marker of ["data-motion", "framer-motion", "motion/react"])
  test(`a chunk carrying ${marker} fails`, () => {
    const dir = bundle({
      "chunks/a/island.js": `x.createElement("span",{"${marker}":""})`,
      "chunks/app.css": cleanCss,
    })
    assert.deepEqual(findMotion(dir), [`chunks/a/island.js: ${marker}`])
  })

test("a stylesheet without the till's no-animation rule fails", () => {
  const dir = bundle({ "chunks/app.css": ".x{transition:all .2s}" })
  assert.deepEqual(findMotion(dir), ["no stylesheet carries the till's no-animation rule"])
})
