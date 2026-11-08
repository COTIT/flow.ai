// Conversation budget: every provider conversation the probe opens is one line in a ledger file.
import { appendFileSync, existsSync, mkdirSync, readFileSync } from "node:fs"
import { dirname } from "node:path"

/** Logs one conversation and returns true, or returns false (logging nothing) once `budget` lines exist. */
export function takeConversation(ledger, budget, label) {
  const used = existsSync(ledger) ? readFileSync(ledger, "utf8").split("\n").filter(Boolean).length : 0
  if (budget !== undefined && used >= budget) return false
  mkdirSync(dirname(ledger), { recursive: true })
  appendFileSync(ledger, `${new Date().toISOString()}\t${label}\n`)
  return true
}
