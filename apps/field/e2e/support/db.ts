import { execFileSync } from 'node:child_process'
import { readFileSync, existsSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, resolve } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const repoRoot = resolve(here, '../..')

function loadEnvFile(): Record<string, string> {
  const path = resolve(repoRoot, '.env')
  if (!existsSync(path)) return {}
  const out: Record<string, string> = {}
  for (const line of readFileSync(path, 'utf8').split('\n')) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    const eq = trimmed.indexOf('=')
    if (eq === -1) continue
    out[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim().replace(/^["']|["']$/g, '')
  }
  return out
}

function connection(): { args: string[]; password: string } | null {
  const env = { ...loadEnvFile(), ...process.env }
  const host = env.SUPABASE_DB_HOST
  const port = env.SUPABASE_DB_PORT || '5432'
  const user = env.SUPABASE_DB_USER
  const password = env.SUPABASE_DB_PASSWORD
  if (!host || !user || !password) return null

  return {
    args: [`host=${host} port=${port} dbname=postgres user=${user} sslmode=require`],
    password,
  }
}

/**
 * Runs one statement as `postgres`, returning its output.
 *
 * For the few fixtures the APP CANNOT CREATE. A farmer has no route to a draft
 * request — the equipment form submits straight to `submitted` — so the only
 * draft in the system is seeded, and `app_verify` and the status machine are
 * one-way: submitting the seeded draft would consume it, and `cleanup.sql`
 * removes marked ROWS and cannot undo a column change on a seeded one.
 *
 * So a test that needs a draft creates its own, marked, and cleanup takes it
 * away again. Never use this to assert a result the app should be asked for.
 */
export function sql(statement: string): { ran: boolean; out: string; reason?: string } {
  const conn = connection()
  if (!conn) return { ran: false, out: '', reason: 'no SUPABASE_DB_* connection details' }

  try {
    // `-q` suppresses the command tag, so `returning id` yields the id alone
    // rather than the id followed by `INSERT 0 1`.
    const out = execFileSync(
      'psql',
      [...conn.args, '-v', 'ON_ERROR_STOP=1', '-q', '-At', '-c', statement],
      {
        env: { ...process.env, PGPASSWORD: conn.password, PGCONNECT_TIMEOUT: '20' },
        stdio: 'pipe',
      },
    )
    return { ran: true, out: out.toString().trim() }
  } catch (cause) {
    return { ran: false, out: '', reason: cause instanceof Error ? cause.message : String(cause) }
  }
}

/**
 * Deletes the records the browser suite created.
 *
 * Runs as `postgres`, which bypasses RLS: the schema has no DELETE policies by
 * design, so this cannot be done through the app's own client. Only rows
 * carrying the E2E- marker are matched, so seeded demo data is untouchable
 * from here.
 *
 * The suite setup treats missing credentials and cleanup failures as errors;
 * accepting either would make seeded-result assertions unreliable.
 */
export function cleanupE2eRecords(): { ran: boolean; reason?: string } {
  const env = { ...loadEnvFile(), ...process.env }
  const host = env.SUPABASE_DB_HOST
  const port = env.SUPABASE_DB_PORT || '5432'
  const user = env.SUPABASE_DB_USER
  const password = env.SUPABASE_DB_PASSWORD

  if (!host || !user || !password) {
    return { ran: false, reason: 'no SUPABASE_DB_* connection details' }
  }

  try {
    execFileSync(
      'psql',
      [
        `host=${host} port=${port} dbname=postgres user=${user} sslmode=require`,
        '-v',
        'ON_ERROR_STOP=1',
        '-q',
        '-f',
        resolve(here, 'cleanup.sql'),
      ],
      { env: { ...process.env, PGPASSWORD: password, PGCONNECT_TIMEOUT: '20' }, stdio: 'pipe' },
    )
    return { ran: true }
  } catch (cause) {
    const detail = cause instanceof Error ? cause.message : String(cause)
    return { ran: false, reason: detail }
  }
}
