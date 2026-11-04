// Shared bits of the full-stack harness (STACK.md): ports, paths, the state file, processes.
import { spawn, spawnSync } from "node:child_process"
import { existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import process from "node:process"
import { fileURLToPath } from "node:url"

/** Shifts every stack port, so two checkouts (or a stack beside the fake-API suite) never clash. */
export const OFFSET = Number(process.env.E2E_STACK_OFFSET ?? 0)
export const PORTS = {
  postgres: 55400 + OFFSET,
  api: 8400 + OFFSET,
  shop: 4300 + OFFSET,
  office: 4301 + OFFSET,
  field: 4302 + OFFSET,
  till: 4303 + OFFSET,
  landing: 4304 + OFFSET,
}
export const URLS = Object.fromEntries(
  Object.entries(PORTS)
    .filter(([name]) => name !== "postgres")
    .map(([name, port]) => [name, `http://127.0.0.1:${port}`])
)

export const E2E = dirname(dirname(fileURLToPath(import.meta.url)))
export const ROOT = dirname(dirname(E2E))
export const API_DIR = join(ROOT, "apps/api")
/** Per offset: state, seed logins and logs of one stack. Gitignored, never committed. */
export const DIR = join(E2E, ".stack", String(OFFSET))
export const STATE = join(DIR, "state.json")
export const SEED = join(DIR, "seed.json")
export const LOGS = join(DIR, "logs")

/** Only ever this container: the stack never touches the dev Postgres or any other. */
export const CONTAINER = `genesy-e2e-stack-${OFFSET}`
export const DB = "genesy"
/** Copy of the freshly seeded database, for `stack:reset`. */
export const SEED_DB = "genesy_seed"
export const DATABASE_URL = `postgres://postgres@127.0.0.1:${PORTS.postgres}/${DB}`

export const readState = () => (existsSync(STATE) ? JSON.parse(readFileSync(STATE, "utf8")) : null)
export function writeState(state) {
  mkdirSync(DIR, { recursive: true })
  writeFileSync(STATE, JSON.stringify(state, null, 2))
}
export const clearState = () => rmSync(STATE, { force: true })

/** Runs a command to the end, output inherited; throws on a non-zero exit. */
export function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { stdio: "inherit", ...opts })
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(" ")} exited ${r.status ?? r.signal}`)
  return r
}

/** `run` without blocking the event loop (so the app builds beside it keep their queue moving). */
export const exec = (cmd, args, opts = {}) =>
  new Promise((resolve, reject) =>
    spawn(cmd, args, { stdio: "inherit", ...opts }).on("close", (code) =>
      code === 0 ? resolve() : reject(new Error(`${cmd} ${args.join(" ")} exited ${code}`))
    )
  )

/** Starts a long-running process in its own group (so `stop` takes its children too), logs to a file. */
export function start(name, cmd, args, opts = {}) {
  mkdirSync(LOGS, { recursive: true })
  const log = openSync(join(LOGS, `${name}.log`), "w")
  const child = spawn(cmd, args, { detached: true, stdio: ["ignore", log, log], ...opts })
  child.unref()
  return child.pid
}

export function stop(pid) {
  try {
    process.kill(-pid, "SIGTERM")
  } catch {
    // Already gone.
  }
}

export const alive = (pid) => {
  // kill(0) would signal our own process group: no pid is never alive.
  if (!pid) return false
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

/** Polls `url` until it answers < 500, or throws after `seconds` (pointing at the log). */
export async function waitFor(name, url, seconds = 120) {
  const until = Date.now() + seconds * 1000
  while (Date.now() < until) {
    try {
      const r = await fetch(url, { redirect: "manual" })
      if (r.status < 500) return
    } catch {
      // Not listening yet.
    }
    await new Promise((resolve) => setTimeout(resolve, 500))
  }
  throw new Error(`${name} did not answer ${url} within ${seconds} s (see ${join(LOGS, `${name}.log`)})`)
}

/** SQL inside the stack's own container (the host may have no psql). */
export const psql = (sql) =>
  run("docker", ["exec", CONTAINER, "psql", "-U", "postgres", "-v", "ON_ERROR_STOP=1", "-qc", sql])

/** The API's env for the stack: debug build, fake payments, rate limits out of the way. */
export function apiEnv(state) {
  return {
    ...process.env,
    DATABASE_URL,
    BIND_ADDR: `127.0.0.1:${PORTS.api}`,
    RUST_LOG: process.env.RUST_LOG ?? "genesy_360_api=info,tower_http=warn",
    FRONTEND_URL: URLS.office,
    API_PUBLIC_URL: URLS.api,
    // Nothing listens there: mail jobs fail quietly in the outbox, no mail leaves the machine.
    SMTP_URL: "smtp://127.0.0.1:9",
    SMTP_ALLOW_INSECURE: "true",
    MAIL_FROM: "Genesy e2e <no-reply@e2e.test>",
    PAYMENTS_PROVIDER: "fake",
    GENESY_ALLOW_DEMO_SEED: "1",
    SIGNUP_ENABLED: "true",
    SKIP_BOOT_MIGRATIONS: "true",
    CREDENTIALS_KEY: state.credentialsKey,
    // Every spec logs in from 127.0.0.1: the per-IP limits would lock the suite out.
    RATE_LIMIT_BURST: "10000",
    RATE_LIMIT_REPLENISH_SECS: "1",
    PRIVATE_RATE_LIMIT_BURST: "10000",
    PRIVATE_IP_RATE_LIMIT_BURST: "100000",
    INVITES_PER_HOUR: "10000",
  }
}

export const apiBin = () =>
  join(process.env.CARGO_TARGET_DIR ?? join(API_DIR, "target"), "debug", "genesy-360-api")

export const startApi = (state) => start("api", apiBin(), [], { env: apiEnv(state), cwd: DIR })

/** Each seeded workspace's id, by account (`seed.workspaces.AFM`), from its first member's login. */
export async function workspaceIds(seed) {
  const ids = {}
  for (const [name, app, role] of [["AFM", "office (AFM)", "owner"], ["RUAHA", "office (RUAHA)", "admin"]]) {
    const who = seed.credentials.find((c) => c.app === app && c.role === role)
    const r = await fetch(`${URLS.api}/v1/login`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ email: who.login, password: who.secret }),
    })
    if (!r.ok) throw new Error(`login ${app} ${role}: ${r.status}`)
    ids[name] = (await r.json()).data.workspace_ids[0]
  }
  return ids
}
