// `pnpm --filter @genesy/e2e stack:test [playwright args]`: up, the stack specs, down (CI runs this).
// A stack that was already up stays up afterwards.
import console from "node:console"
import { spawnSync } from "node:child_process"
import process from "node:process"

import { down } from "./down.mjs"
import { up } from "./up.mjs"
import { alive, E2E, readState } from "./stack.mjs"

const t0 = Date.now()
const wasUp = alive(readState()?.pids.api)
let code = 1
try {
  await up()
  const r = spawnSync("pnpm", ["exec", "playwright", "test", "-c", "playwright.stack.config.ts", ...process.argv.slice(2)], {
    cwd: E2E,
    stdio: "inherit",
  })
  code = r.status ?? 1
} catch (err) {
  console.error(`stack: ${err.message ?? err}`)
} finally {
  if (!wasUp) down()
}
console.log(`stack: total ${((Date.now() - t0) / 1000).toFixed(0)} s`)
process.exit(code)
