// Reads what `genesy-360-api seed-demo` prints once (its logins exist nowhere else).

/**
 * `app | role | login | secret | path` rows become `credentials`; `label: value` lines after
 * the table become `notes` (storefront app key, passport, sample paths).
 */
export function parseSeed(stdout) {
  const credentials = []
  const notes = {}
  for (const line of stdout.split("\n")) {
    const cells = line.split(" | ").map((c) => c.trim())
    if (cells.length === 5) {
      const [app, role, login, secret, path] = cells
      if (app !== "app") credentials.push({ app, role, login, secret, path })
      continue
    }
    const at = line.indexOf(": ")
    if (at > 0) notes[line.slice(0, at).trim()] = line.slice(at + 2).trim()
  }
  if (credentials.length === 0) throw new Error(`seed-demo printed no logins:\n${stdout}`)
  return { credentials, notes }
}
