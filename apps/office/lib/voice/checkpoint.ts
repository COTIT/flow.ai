import "server-only"
import { createHash } from "node:crypto"
import pg from "pg"
import { PostgresSaver } from "@langchain/langgraph-checkpoint-postgres"
import { VoiceError } from "./server"
import { voiceSetting } from "./settings"

const runtime = globalThis as typeof globalThis & {
  __genesyVoiceCheckpoint?: {
    pool?: pg.Pool
    saver?: PostgresSaver
    writers: number
  }
}
const shared = (runtime.__genesyVoiceCheckpoint ??= { writers: 0 })

/** Restricted checkpoint connection; SQLx migration owns setup and grants. */
async function checkpointPool(): Promise<pg.Pool> {
  if (!shared.pool) {
    const connectionString = await voiceSetting("ASSISTANT_CHECKPOINT_URL")
    if (!connectionString)
      throw new VoiceError("Assistant workflow storage is not configured.", 503)
    shared.pool ??= new pg.Pool({
      connectionString,
      max: 4,
      idleTimeoutMillis: 10000,
      connectionTimeoutMillis: 5000,
      allowExitOnIdle: true,
    })
  }
  return shared.pool
}

export async function voiceCheckpointer(): Promise<PostgresSaver> {
  if (!shared.saver) {
    const db = await checkpointPool()
    const version = await db.query<{ version: number }>(
      "SELECT max(v) AS version FROM assistant_checkpoint.checkpoint_migrations"
    )
    if (version.rows[0]?.version !== 5)
      throw new VoiceError(
        "Assistant checkpoint schema requires migration.",
        503
      )
    shared.saver ??= new PostgresSaver(db, undefined, {
      schema: "assistant_checkpoint",
    })
  }
  return shared.saver
}

/** Serialize graph writers across replicas; Rust's action claim separately guards effects. */
export async function withVoiceThread<T>(
  sessionId: string,
  fn: () => Promise<T>
): Promise<T> {
  if (shared.writers >= 2)
    throw new VoiceError(
      "Two assistant tasks are already being processed. Try again shortly.",
      429
    )
  shared.writers++
  try {
    const db = await checkpointPool()
    const client = await db.connect()
    const lock = createHash("sha256")
      .update(`assistant:${sessionId}`)
      .digest()
      .readBigInt64BE()
      .toString()
    let acquired = false
    try {
      const result = await client.query<{ locked: boolean }>(
        "SELECT pg_try_advisory_lock($1::bigint) AS locked",
        [lock]
      )
      acquired = result.rows[0]?.locked === true
      if (!acquired)
        throw new VoiceError(
          "This assistant task is busy. Wait for its current step.",
          409
        )
      return await fn()
    } finally {
      try {
        if (acquired)
          await client.query("SELECT pg_advisory_unlock($1::bigint)", [lock])
      } finally {
        client.release()
      }
    }
  } finally {
    shared.writers--
  }
}
