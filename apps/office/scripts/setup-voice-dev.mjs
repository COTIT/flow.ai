import assert from "node:assert/strict"
import { readFile, writeFile, chmod } from "node:fs/promises"
import { randomBytes } from "node:crypto"
import { parseEnv } from "node:util"
import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import pg from "pg"

const officePath = fileURLToPath(new URL("../.env.local", import.meta.url))
const apiPath = fileURLToPath(new URL("../../api/.env", import.meta.url))
const office = parseEnv(await readFile(officePath, "utf8"))
const api = parseEnv(await readFile(apiPath, "utf8"))
const database = process.argv[2]
assert(database && /^genesy_assistant_checks_[a-zA-Z0-9_]+$/.test(database), "Pass an existing isolated assistant check database")
const admin = new URL(api.DATABASE_URL)
assert(["127.0.0.1", "localhost"].includes(admin.hostname), "Local setup refuses remote databases")
admin.pathname = "/" + database
const role = "assistant_checkpoint_local"
const existing = office.ASSISTANT_CHECKPOINT_URL ? new URL(office.ASSISTANT_CHECKPOINT_URL) : undefined
const db = new pg.Client({ connectionString: admin.href })
await db.connect()
try {
  const result = await db.query("SELECT rolcanlogin FROM pg_roles WHERE rolname=$1", [role])
  assert(!result.rows[0]?.rolcanlogin || existing?.username === role, "Existing local checkpoint login belongs to another setup; credentials left unchanged")
  const env = { ...process.env, PGHOST: admin.hostname, PGPORT: admin.port || "5432", PGUSER: decodeURIComponent(admin.username), PGPASSWORD: decodeURIComponent(admin.password), PGDATABASE: database }
  const sql = fileURLToPath(new URL("../../../deploy/assistant-role.sql", import.meta.url))
  for (const mode of ["bootstrap"]) {
    const check = spawnSync("psql", ["--no-psqlrc", "--set", "ON_ERROR_STOP=1", "--set", "checkpoint_role=" + role, "--set", "checkpoint_mode=" + mode, "--file", sql], { env, encoding: "utf8" })
    assert(check.status === 0, "Checkpoint role setup failed: " + check.stderr)
  }
  const password = existing?.username === role ? decodeURIComponent(existing.password) : randomBytes(32).toString("hex")
  assert(/^[a-zA-Z0-9]+$/.test(password))
  await db.query("SET log_statement = 'none'")
  await db.query("ALTER ROLE assistant_checkpoint_local LOGIN PASSWORD '" + password + "'")
  const checkpoint = new URL(admin.href)
  checkpoint.username = role
  checkpoint.password = password
  const bridge = office.ASSISTANT_BRIDGE_SECRET?.length >= 32 ? office.ASSISTANT_BRIDGE_SECRET : randomBytes(32).toString("hex")
  async function update(path, values) {
    let content = await readFile(path, "utf8")
    for (const [key, value] of Object.entries(values)) {
      assert(!value.includes("\n") && !value.includes("\r"))
      const line = key + "=" + value
      const pattern = new RegExp("^" + key + "=.*$", "m")
      content = pattern.test(content) ? content.replace(pattern, () => line) : content.trimEnd() + "\n" + line + "\n"
    }
    await writeFile(path, content)
    await chmod(path, 0o600)
  }
  // Provisioning leaves admission off until workspace rollout is reviewed.
  await update(officePath, { ASSISTANT_ENABLED: "false", ASSISTANT_BRIDGE_SECRET: bridge, ASSISTANT_CHECKPOINT_URL: checkpoint.href })
  await update(apiPath, { DATABASE_URL: admin.href, GENESY_DATABASE_URL: admin.href, ASSISTANT_BRIDGE_SECRET: bridge, ELEVENLABS_API_KEY: office.ELEVENLABS_API_KEY })
  const verify = spawnSync("psql", ["--no-psqlrc", "--set", "ON_ERROR_STOP=1", "--set", "checkpoint_role=" + role, "--set", "checkpoint_mode=verify", "--file", sql], { env, encoding: "utf8" })
  assert(verify.status === 0, "Checkpoint role verification failed: " + verify.stderr)
  const runtime = new pg.Client({ connectionString: checkpoint.href })
  await runtime.connect()
  try {
    const version = await runtime.query("SELECT max(v) AS version FROM assistant_checkpoint.checkpoint_migrations")
    assert.equal(version.rows[0].version, 5)
    await assert.rejects(runtime.query("SELECT id FROM public.workspaces LIMIT 1"), { code: "42501" })
    await assert.rejects(runtime.query("CREATE TABLE assistant_checkpoint.forbidden(id int)"), { code: "42501" })
  } finally { await runtime.end() }
  console.log(JSON.stringify({ database, role, checkpointVersion: 5, businessReadDenied: true, runtimeDdlDenied: true, privateConfigurationWritten: true }))
} finally { await db.end() }
