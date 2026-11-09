// `pnpm --filter @genesy/e2e stack:up`: throwaway Postgres, migrated + seeded, the API and all
// five apps pointed at it (STACK.md). Anything that fails tears down what was started.
import console from "node:console"
import { randomBytes } from "node:crypto"
import { spawn, spawnSync } from "node:child_process"
import { mkdirSync, openSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import process from "node:process"

import { down } from "./down.mjs"
import { parseSeed } from "./seed.mjs"
import {
  API_DIR, CONTAINER, DATABASE_URL, DB, DIR, E2E, FILES_BUCKET, FILES_CONTAINER, FILES_IMAGE, FILES_ORIGIN,
  LOGS, PORTS, ROOT, SEED, SEED_DB, URLS,
  alive, apiBin, apiEnv, exec, psql, readState, run, start, startApi, startMail, stop, voiceEnv, voiceRole, waitFor, workspaceIds, writeState,
} from "./stack.mjs"

const t0 = Date.now()
/** App builds still running: stopped too when `stack:up` fails. */
const building = new Set()
const lap = (what) => console.log(`stack: ${what} (${((Date.now() - t0) / 1000).toFixed(0)} s)`)

/** Office needs package build preparation; other stack builds retain their isolated entrypoints. */
export function nextBuildArgs(name) {
  return ["--filter", `@genesy/${name}`, ...(name === "office" ? ["build"] : ["exec", "next", "build"])]
}

export async function up() {
  const existing = readState()
  if (existing && alive(existing.pids.api)) {
    console.log(`stack: already up (offset ${existing.offset}); stack:down first to rebuild`)
    return existing
  }
  if (existing) down()

  mkdirSync(LOGS, { recursive: true })
  const state = {
    offset: Number(process.env.E2E_STACK_OFFSET ?? 0),
    ports: PORTS,
    urls: URLS,
    container: CONTAINER,
    // Random per stack, kept for API restarts: the seed's encrypted credentials need the same key.
    credentialsKey: randomBytes(32).toString("hex"),
    // The file store's key, random per stack (state.json is gitignored).
    files: { keyId: `GK${randomBytes(12).toString("hex")}`, secret: randomBytes(32).toString("hex") },
    pids: {},
  }
  if (process.env.VOICE === "1") {
    if (!process.env.ELEVENLABS_API_KEY || !process.env.ELEVENLABS_AGENT_ID)
      throw new Error("VOICE=1 needs ELEVENLABS_API_KEY and ELEVENLABS_AGENT_ID in the environment")
    // Random per stack, like the keys above; the provider key itself is never written here.
    state.voice = { bridgeSecret: randomBytes(32).toString("hex"), checkpointPassword: randomBytes(24).toString("hex") }
  }
  writeState(state)

  const api = `${URLS.api}/v1`
  const apps = {
    office: { env: { API_URL: api, FILES_ORIGIN } },
    shop: { env: { API_URL: api, SITE_URL: URLS.shop, FILES_ORIGIN } },
    till: { env: { API_URL: api } },
    landing: { env: { SITE_URL: URLS.landing, NEXT_PUBLIC_SHOP_URL: URLS.shop } },
  }
  // Next bakes the /v1 rewrite into the build, so these builds are the stack's own (.next-stack).
  const nextEnv = (name) => ({ ...process.env, ...apps[name].env, NEXT_DIST_DIR: ".next-stack" })
  const field = { ...process.env, API_URL: api, VITE_FEATURE_RESOURCES: "on" }
  const builds = [
    ...Object.keys(apps).map((name) => [name, nextBuildArgs(name), nextEnv(name)]),
    ["field", ["--filter", "@genesy/field", "exec", "vite", "build", "--outDir", "dist-stack"], field],
  ]
  // Two at a time, beside the cargo build and the seed: a 2-core runner gains nothing from more.
  // Logs go to logs/build-<app>.log.
  const queue = [...builds]
  const built = Promise.all([0, 1].map(async () => {
    for (let b = queue.shift(); b; b = queue.shift()) {
      const [name, args, env] = b
      const log = openSync(join(LOGS, `build-${name}.log`), "w")
      const child = spawn("pnpm", args, { cwd: ROOT, env, stdio: ["ignore", log, log], detached: true })
      building.add(child)
      const code = await new Promise((resolve) => child.on("close", resolve))
      building.delete(child)
      if (code !== 0) throw new Error(`build ${name} failed (see ${join(LOGS, `build-${name}.log`)})`)
    }
  }))
  built.catch(() => {}) // awaited below; a failure must not crash the process before then

  // Postgres in RAM, no crash safety: nothing outlives the stack.
  run("docker", [
    "run", "-d", "--rm", "--name", CONTAINER, "--label", "genesy-e2e-stack=1",
    "-p", `127.0.0.1:${PORTS.postgres}:5432`, "--tmpfs", "/var/lib/postgresql:rw,size=1g",
    "-e", "POSTGRES_HOST_AUTH_METHOD=trust", "-e", `POSTGRES_DB=${DB}`,
    "postgres:18-alpine", "postgres", "-c", "fsync=off", "-c", "synchronous_commit=off",
    "-c", "full_page_writes=off", "-c", "max_connections=200",
  ], { stdio: ["ignore", "ignore", "inherit"] })
  // The image's entrypoint restarts Postgres once after init: wait for the real one.
  for (let i = 0; ; i++) {
    const r = spawnSync("docker", ["exec", CONTAINER, "pg_isready", "-U", "postgres", "-h", "127.0.0.1", "-d", DB])
    const logs = spawnSync("docker", ["logs", CONTAINER], { encoding: "utf8" })
    if (r.status === 0 && /PostgreSQL init process complete/.test(logs.stderr + logs.stdout)) break
    if (i > 120) throw new Error("Postgres did not start")
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  lap("postgres up")

  // Garage in RAM: a local S3 store so uploads never leave the machine; throwaway secrets.
  run("docker", [
    "run", "-d", "--rm", "--name", FILES_CONTAINER, "--label", "genesy-e2e-stack=1",
    "-p", `127.0.0.1:${PORTS.files}:3900`, "--tmpfs", "/var/lib/garage:rw,size=256m",
    "-v", `${join(E2E, "stack/garage.toml")}:/etc/garage.toml:ro`,
    "-e", `GARAGE_RPC_SECRET=${randomBytes(32).toString("hex")}`,
    "-e", `GARAGE_DEFAULT_ACCESS_KEY=${state.files.keyId}`,
    "-e", `GARAGE_DEFAULT_SECRET_KEY=${state.files.secret}`,
    "-e", `GARAGE_DEFAULT_BUCKET=${FILES_BUCKET}`,
    FILES_IMAGE, "/garage", "server", "--single-node", "--default-bucket",
  ], { stdio: ["ignore", "ignore", "inherit"] })
  // An unsigned GET answers 403 once the bucket and key exist.
  for (let i = 0; ; i++) {
    const status = await fetch(`${FILES_ORIGIN}/${FILES_BUCKET}/ready`).then((r) => r.status, () => 0)
    if (status === 403) break
    if (i > 60) throw new Error("Garage did not start")
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  lap("files up")

  // CI's settings (ci.yml env): no debug info, no incremental cache. Differing ones rebuild everything.
  await exec("cargo", ["build", "--bin", "genesy-360-api"], {
    cwd: API_DIR,
    env: { ...process.env, SQLX_OFFLINE: "true", CARGO_INCREMENTAL: "0", CARGO_PROFILE_DEV_DEBUG: "0" },
  })
  lap("api built (debug)")

  state.pids.mail = startMail()
  writeState(state)
  run(apiBin(), ["migrate"], { env: { ...process.env, DATABASE_URL }, cwd: DIR })
  if (state.voice) voiceRole(state)
  const seeded = spawnSync(apiBin(), ["seed-demo"], {
    // Tracing writes to stdout too: keep it off the printed table.
    env: { ...apiEnv(state), RUST_LOG: "error" }, cwd: DIR, encoding: "utf8" })
  if (seeded.status !== 0) throw new Error(`seed-demo failed:\n${seeded.stdout}\n${seeded.stderr}`)
  const seed = parseSeed(seeded.stdout)
  writeFileSync(SEED, JSON.stringify(seed, null, 2))
  psql(`CREATE DATABASE ${SEED_DB} TEMPLATE ${DB}`)
  lap(`seeded (${seed.credentials.length} logins in ${SEED})`)

  // Read at request time, not build time: only the server needs the storefront key.
  apps.shop.env.STOREFRONT_APP_KEY = seed.notes["storefront app key (shop container)"]
  state.pids.api = startApi(state)
  writeState(state)
  await waitFor("api", `${URLS.api}/healthz`)
  seed.workspaces = await workspaceIds(seed)
  writeFileSync(SEED, JSON.stringify(seed, null, 2))
  // VOICE_WORKSPACES picks seeded accounts by name (default AFM); nothing else is allowlisted.
  if (state.voice) {
    const voiceIds = (process.env.VOICE_WORKSPACES ?? "AFM").split(",").map((name) => {
      const id = seed.workspaces[name.trim()]
      if (!id) throw new Error(`VOICE_WORKSPACES: no seeded workspace "${name}"`)
      return id
    })
    Object.assign(apps.office.env, voiceEnv(state, voiceIds))
  }
  lap("api up")

  await built
  lap("apps built")

  for (const name of Object.keys(apps)) {
    state.pids[name] = start(name, "pnpm", [
      "--filter", `@genesy/${name}`, "exec", "next", "start", "--port", String(PORTS[name]), "--hostname", "127.0.0.1",
    ], { cwd: ROOT, env: nextEnv(name) })
  }
  state.pids.field = start("field", "pnpm", [
    "--filter", "@genesy/field", "exec", "vite", "preview", "--outDir", "dist-stack",
    "--port", String(PORTS.field), "--host", "127.0.0.1", "--strictPort",
  ], { cwd: ROOT, env: field })
  writeState(state)
  await Promise.all([
    waitFor("office", `${URLS.office}/login`),
    waitFor("shop", URLS.shop),
    waitFor("till", `${URLS.till}/setup`),
    waitFor("landing", URLS.landing),
    waitFor("field", `${URLS.field}/login`),
  ])
  lap(`up: ${Object.entries(URLS).map(([n, u]) => `${n} ${u}`).join(", ")}`)
  return state
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const bail = (err) => {
    console.error(`stack: ${err.message ?? err}`)
    for (const child of building) stop(child.pid)
    down()
    process.exit(1)
  }
  process.on("SIGINT", () => bail("interrupted"))
  process.on("SIGTERM", () => bail("terminated"))
  up().catch(bail)
}
