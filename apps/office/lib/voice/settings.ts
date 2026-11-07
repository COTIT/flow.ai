import "server-only"
import { readFile } from "node:fs/promises"

/** Runtime-only settings support Compose secret files without build-time credentials. */
export async function voiceSetting(name: string): Promise<string> {
  const plain = process.env[name]
  const file = process.env[`${name}_FILE`]
  if (plain && file) throw new Error(`Conflicting ${name} configuration`)
  if (!file) return plain ?? ""
  try { return (await readFile(file, "utf8")).replace(/\r?\n$/, "") }
  catch { throw new Error(`${name} file is unreadable`) }
}
