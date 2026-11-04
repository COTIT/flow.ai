// `pnpm --filter @genesy/e2e stack:down`: stops this offset's processes and removes its container.
import console from "node:console"
import { spawnSync } from "node:child_process"
import process from "node:process"

import { CONTAINER, clearState, readState, stop } from "./stack.mjs"

export function down() {
  const state = readState()
  for (const pid of Object.values(state?.pids ?? {})) stop(pid)
  // Only the stack's own container, by its exact name; gone already is fine.
  spawnSync("docker", ["rm", "-f", CONTAINER], { stdio: "ignore" })
  clearState()
  console.log(`stack: down (${CONTAINER} removed)`)
}

if (import.meta.url === `file://${process.argv[1]}`) down()
