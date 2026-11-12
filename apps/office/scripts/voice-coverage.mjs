// G7 of docs/genesy-flow/OFFICE_VOICE_COVERAGE.md: how voice reaches each Office area, printed in CI.
// `pnpm --filter @genesy/office voice:coverage`; appends to $GITHUB_STEP_SUMMARY when set.
// Report only: the guards that fail live in lib/voice/office-parity.test.ts and keywords.test.ts.
import { appendFileSync, readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"

import { officeScreenOps, policy } from "./office-parity-scan.mjs"

/** The Office area a source file belongs to: the first route segment, or its folder. */
export function areaOf(file) {
  const route = file.match(/^app\/\(app\)\/([^/]+)\//)
  if (route) return route[1]
  if (/^app\/\(app\)\/[^/]+$/.test(file)) return "overview"
  if (file === "@genesy/api-client/storefront-review") return "settings"
  if (file.startsWith("@genesy/api-client/")) return "sign-in and session"
  return file.split("/")[file.startsWith("app/") ? 1 : 0]
}

/** One row per area: actions its screens call, and how many voice runs, hands off, or leaves to Office. */
export function coverage(ops, operations) {
  const rows = new Map()
  for (const [op, files] of ops)
    for (const area of new Set([...files].map(areaOf))) {
      const row = rows.get(area) ?? {
        area,
        actions: 0,
        voice: 0,
        handoff: 0,
        officeOnly: 0,
      }
      const disposition = operations[op]?.disposition
      row.actions++
      if (disposition === "execute") row.voice++
      else if (disposition === "secure_ui") row.handoff++
      else row.officeOnly++
      rows.set(area, row)
    }
  return [...rows.values()].sort((a, b) => a.area.localeCompare(b.area))
}

export function coverageMarkdown(rows, totals) {
  return [
    "## Office voice coverage",
    "",
    "| Area | Actions | Voice finds and runs | Secure hand-off | Office only |",
    "|---|---|---|---|---|",
    ...rows.map(
      (r) =>
        `| ${r.area} | ${r.actions} | ${r.voice} | ${r.handoff} | ${r.officeOnly} |`
    ),
    "",
    `${totals.execute} executable operations: ${totals.withWords} with trigger words, ${totals.withPicture} drawn as a table or chart.`,
    "",
  ].join("\n")
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const voice = new URL("../lib/voice/", import.meta.url)
  const keywords = JSON.parse(
    readFileSync(new URL("keywords.json", voice), "utf8")
  )
  // shortcut: reads view ids from the source text of views.ts; switch to an exported list if its shape changes.
  const views = readFileSync(
    new URL("../../../packages/voice/src/lib/views.ts", import.meta.url),
    "utf8"
  )
  const lists = views.match(/const LISTS[^}]*\}/)?.[0] ?? ""
  const drawn = new Set([
    ...[...lists.matchAll(/^\s+(\w+):/gm)].map((m) => m[1]),
    ...[...views.matchAll(/operationId === "(\w+)"/g)].map((m) => m[1]),
  ])
  const execute = Object.keys(policy).filter(
    (id) => policy[id].disposition === "execute"
  )
  const md = coverageMarkdown(coverage(officeScreenOps().ops, policy), {
    execute: execute.length,
    withWords: execute.filter((id) => id in keywords).length,
    withPicture: execute.filter((id) => drawn.has(id)).length,
  })
  process.stdout.write(md)
  // eslint-disable-next-line turbo/no-undeclared-env-vars -- CI's summary file; this script is not a turbo task
  const summary = process.env.GITHUB_STEP_SUMMARY
  if (summary) appendFileSync(summary, md)
}
