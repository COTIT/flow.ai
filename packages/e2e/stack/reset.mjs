// `pnpm --filter @genesy/e2e stack:reset`: back to the freshly seeded data in about a second.
// Same logins and keys as before (seed.json stays valid); the apps keep running.
import console from "node:console"
import process from "node:process"

import { alive, DB, SEED_DB, URLS, psql, readState, startApi, stop, waitFor, writeState } from "./stack.mjs"

const state = readState()
if (!state) {
  console.error("stack: not up (stack:up first)")
  process.exit(1)
}
stop(state.pids.api)
while (alive(state.pids.api)) await new Promise((resolve) => setTimeout(resolve, 100))
psql(`DROP DATABASE ${DB} WITH (FORCE)`)
psql(`CREATE DATABASE ${DB} TEMPLATE ${SEED_DB}`)
state.pids.api = startApi(state)
writeState(state)
await waitFor("api", `${URLS.api}/healthz`)
console.log("stack: reset to the seeded data")
