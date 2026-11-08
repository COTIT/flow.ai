import "server-only"
import pg from "pg"
import { assistantEnabled, assistantWorkspaces } from "./admission"
import { readBounded } from "./bridge"
import { voiceSetting } from "./settings"
import { voiceCheckpointer } from "./checkpoint"

type Check = "configuration" | "provider" | "checkpoint" | "backend"
export type AssistantReadiness = {
  enabled: boolean
  ready: boolean
  checks?: Record<Check, boolean>
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error("Invalid provider configuration")
  return value as Record<string, unknown>
}

/** Unknown/default fallback chains cannot prove that every selected model accepts files. */
export function validateNativeMultimodal(
  agent: unknown,
  catalog: unknown
): void {
  const configuration = object(object(agent).conversation_config)
  const files = object(object(configuration.conversation).file_input)
  if (
    files.enabled !== true ||
    !Number.isInteger(files.max_files_in_memory) ||
    Number(files.max_files_in_memory) < 1 ||
    Number(files.max_files_in_memory) > 10 ||
    !Number.isInteger(files.max_files_per_conversation) ||
    Number(files.max_files_per_conversation) > 10 ||
    Number(files.max_files_per_conversation) < Number(files.max_files_in_memory)
  )
    throw new Error("File input is not safely configured")
  const rows = object(catalog).llms
  if (!Array.isArray(rows))
    throw new Error("Model capabilities are unavailable")
  const models = new Map(
    rows.map((row) => {
      const entry = object(row)
      return [entry.llm, entry]
    })
  )
  const primary = object(object(configuration.agent).prompt)
  function validate(prompt: Record<string, unknown>) {
    const merged = { ...primary, ...prompt }
    const backup = object(merged.backup_llm_config)
    if (
      merged.custom_llm ||
      backup.preference !== "override" ||
      !Array.isArray(backup.order) ||
      !backup.order.length ||
      backup.order.length > 10
    )
      throw new Error("An explicit native fallback chain is required")
    for (const id of [merged.llm, ...backup.order]) {
      const model = models.get(id)
      if (
        typeof id !== "string" ||
        id === "custom-llm" ||
        !model ||
        model.supports_image_input !== true ||
        model.supports_document_input !== true
      )
        throw new Error(
          "A selected model does not support images and documents"
        )
    }
  }
  validate(primary)
  function overrides(value: unknown, depth = 0) {
    if (depth > 12) throw new Error("Provider overrides are too deeply nested")
    if (!value || typeof value !== "object") return
    if (!Array.isArray(value)) {
      const entry = object(value)
      if (
        typeof entry.llm === "string" ||
        "backup_llm_config" in entry ||
        entry.custom_llm
      )
        validate(entry)
    }
    for (const child of Object.values(value)) overrides(child, depth + 1)
  }
  overrides(configuration.language_presets)
}

async function providerConfiguration(
  key: string,
  agent: string
): Promise<void> {
  async function get(path: string) {
    const response = await fetch("https://api.elevenlabs.io" + path, {
      headers: { "xi-api-key": key },
      cache: "no-store",
      redirect: "error",
      signal: AbortSignal.timeout(3000),
    })
    if (!response.ok) throw new Error("Provider configuration unavailable")
    return JSON.parse(
      new TextDecoder().decode(await readBounded(response.body, 2_000_000))
    ) as unknown
  }
  const [configured, models] = await Promise.all([
    get(`/v1/convai/agents/${encodeURIComponent(agent)}`),
    get("/v1/convai/llm/list"),
  ])
  if (object(configured).agent_id !== agent)
    throw new Error("Provider configuration belongs to another agent")
  validateNativeMultimodal(configured, models)
}

async function checkpointReady(connectionString: string): Promise<boolean> {
  const client = new pg.Client({
    connectionString,
    connectionTimeoutMillis: 2000,
    query_timeout: 2000,
    statement_timeout: 2000,
  })
  try {
    await client.connect()
    const result = await client.query<{
      version: number
      restricted: boolean
      checkpoint_access: boolean
    }>(`
      SELECT (SELECT max(v) FROM assistant_checkpoint.checkpoint_migrations) AS version,
        (SELECT NOT (rolsuper OR rolcreatedb OR rolcreaterole OR rolreplication OR rolbypassrls)
          FROM pg_roles WHERE rolname=current_user)
        AND current_user ~ '^assistant_checkpoint_(local|staging|production)$'
        AND pg_has_role(current_user,'assistant_checkpoint_runtime','MEMBER')
        AND NOT has_database_privilege(current_user,current_database(),'CREATE')
        AND NOT EXISTS (SELECT FROM pg_namespace WHERE has_schema_privilege(current_user,oid,'CREATE'))
        AND NOT EXISTS (SELECT FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
          WHERE n.nspname='public' AND c.relkind IN ('r','p','v','m','S','f') AND
          CASE WHEN c.relkind='S' THEN has_sequence_privilege(current_user,c.oid,'SELECT,UPDATE,USAGE')
          ELSE has_table_privilege(current_user,c.oid,'SELECT,INSERT,UPDATE,DELETE,TRUNCATE,REFERENCES,TRIGGER')
            OR has_any_column_privilege(current_user,c.oid,'SELECT,INSERT,UPDATE,REFERENCES') END) AS restricted,
        has_schema_privilege(current_user,'assistant_checkpoint','USAGE')
        AND has_table_privilege(current_user,'assistant_checkpoint.checkpoint_migrations','SELECT')
        AND NOT has_table_privilege(current_user,'assistant_checkpoint.checkpoint_migrations','INSERT,UPDATE,DELETE,TRUNCATE')
        AND (SELECT bool_and(has_table_privilege(current_user,'assistant_checkpoint.' || relation,privilege))
          FROM unnest(ARRAY['checkpoints','checkpoint_blobs','checkpoint_writes']) AS relation
          CROSS JOIN unnest(ARRAY['SELECT','INSERT','UPDATE','DELETE']) AS privilege) AS checkpoint_access`)
    const row = result.rows[0]
    return (
      row?.version === 5 &&
      row.restricted === true &&
      row.checkpoint_access === true
    )
  } finally {
    await client.end()
  }
}

async function inspect(): Promise<AssistantReadiness> {
  const checks: Record<Check, boolean> = {
    configuration: false,
    provider: false,
    checkpoint: false,
    backend: false,
  }
  try {
    const [key, agent, bridge, checkpoint, api] = await Promise.all(
      [
        "ELEVENLABS_API_KEY",
        "ELEVENLABS_AGENT_ID",
        "ASSISTANT_BRIDGE_SECRET",
        "ASSISTANT_CHECKPOINT_URL",
        "API_URL",
      ].map(voiceSetting)
    )
    const backend = new URL(api || "http://localhost:8080/v1")
    const database = new URL(checkpoint!)
    if (
      !key ||
      !agent ||
      !/^[a-zA-Z0-9_-]{1,200}$/.test(agent) ||
      !bridge ||
      bridge.length < 32 ||
      !["http:", "https:"].includes(backend.protocol) ||
      backend.username ||
      backend.password ||
      !["postgres:", "postgresql:"].includes(database.protocol) ||
      !database.hostname
    )
      throw new Error("Missing required configuration")
    await assistantWorkspaces()
    checks.configuration = true
    const results = await Promise.allSettled([
      providerConfiguration(key, agent),
      checkpointReady(checkpoint!),
      fetch(new URL("/healthz", backend).href, {
        cache: "no-store",
        redirect: "error",
        signal: AbortSignal.timeout(2000),
      }).then((response) => response.status === 200),
    ])
    checks.provider = results[0].status === "fulfilled"
    checks.checkpoint = results[1].status === "fulfilled" && results[1].value
    checks.backend = results[2].status === "fulfilled" && results[2].value
    // Warm the shared pool before the first review; failures stay private.
    if (checks.checkpoint) await voiceCheckpointer().catch(() => undefined)
  } catch {
    /* Configuration details stay private; readiness returns booleans only. */
  }
  return { enabled: true, ready: Object.values(checks).every(Boolean), checks }
}

type ReadinessRuntime = {
  cached?: { expires: number; result: AssistantReadiness }
  pending?: Promise<AssistantReadiness>
}
const runtimeKey = Symbol.for("genesy.office.voice.readiness.v1")
function readinessRuntime(): ReadinessRuntime {
  const shared = globalThis as typeof globalThis & {
    [runtimeKey]?: ReadinessRuntime
  }
  return (shared[runtimeKey] ??= {})
}
/** Process-wide single-flight keeps route bundles within one readiness connection budget. */
export async function assistantReadiness(
  useCache = true
): Promise<AssistantReadiness> {
  try {
    if (!(await assistantEnabled())) return { enabled: false, ready: true }
  } catch {
    return { enabled: false, ready: false }
  }
  if (!useCache) return inspect()
  const state = readinessRuntime()
  if (state.cached && state.cached.expires > Date.now())
    return state.cached.result
  state.pending ??= inspect()
    .then((result) => {
      state.cached = {
        result,
        expires: Date.now() + (result.ready ? 10000 : 1000),
      }
      return result
    })
    .finally(() => {
      state.pending = undefined
    })
  return state.pending
}
