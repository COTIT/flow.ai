#!/usr/bin/env node
// Developer-only voice gap triage and trigger-word heal. Never pushes, merges or opens a PR.
//
//   psql "$DATABASE_URL" -At -v triage=1 -v days=7 -f apps/api/scripts/voice-gaps.sql \
//     | pnpm --filter @genesy/office voice:triage -- --days 7 [--out report.md]
//   ... | pnpm --filter @genesy/office voice:heal -- [--dry-run]
//   psql "$DATABASE_URL" -At -v demand=1 -v days=7 -f apps/api/scripts/voice-gaps.sql \
//     | pnpm --filter @genesy/office voice:demand -- --days 7 [--out demand]   (writes demand.json + demand.md)
//
// Flags: --input <file> (instead of stdin), --min-gaps 2, --min-workspaces 2, --solo-gaps 5.
import { execFileSync } from "node:child_process"
import { readFileSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { createViteServer } from "vitest/node"

const office = fileURLToPath(new URL("..", import.meta.url))
const voice = `${office}lib/voice/`
const TESTS = [
  "lib/voice/keywords.test.ts",
  "lib/voice/discovery.test.ts",
  "lib/voice/gap-heal.test.ts",
  "lib/voice/catalog.test.ts",
]

const [command, ...rest] = process.argv.slice(2).filter((a) => a !== "--")
const flag = (name, fallback) => {
  const at = rest.indexOf(`--${name}`)
  return at === -1 ? fallback : rest[at + 1]
}
const number = (name, fallback) => {
  const value = Number(flag(name, fallback))
  if (!Number.isInteger(value) || value < 1)
    throw new Error(`--${name} must be a whole number of at least 1`)
  return value
}
if (!["triage", "heal", "demand"].includes(command)) {
  console.error(
    "usage: voice-heal.mjs triage|heal|demand [--input file] [--days 7] [--out file] [--dry-run]"
  )
  process.exit(2)
}

const raw = readFileSync(flag("input", 0), "utf8").trim()
const rows = raw ? JSON.parse(raw) : []
if (!Array.isArray(rows))
  throw new Error(
    `expected a JSON array from voice-gaps.sql -v ${command === "demand" ? "demand" : "triage"}=1`
  )
const thresholds = {
  minGaps: number("min-gaps", 2),
  minWorkspaces: number("min-workspaces", 2),
  soloGaps: number("solo-gaps", 5),
}

// catalog.ts is TypeScript with JSON imports; Vitest's Vite server loads it with the test aliases.
const server = await createViteServer({
  configFile: `${office}vitest.config.ts`,
  root: office,
  server: { middlewareMode: true, hmr: false },
  appType: "custom",
  logLevel: "error",
})
const keywordsFile = `${voice}keywords.json`
const healsFile = `${voice}gap-heal.fixture.json`
let plan, markdown, items
try {
  const { clusterGaps, classify, planHeal, report } =
    await server.ssrLoadModule("/lib/voice/gap-triage.ts")
  if (command === "demand") {
    // Demand reads only k-anonymous aggregates; fromDemandRow refuses per-row triage input.
    const { demand, demandReport, fromDemandRow } = await server.ssrLoadModule(
      "/lib/voice/gap-demand.ts"
    )
    items = demand(
      rows.map(fromDemandRow).map(classify),
      JSON.parse(readFileSync(`${voice}gap-demand.resolved.json`, "utf8"))
    )
    markdown = demandReport(items, {
      days: number("days", 7),
      minWorkspaces: number("k", 3),
    })
  } else {
    const triaged = clusterGaps(rows).map(classify)
    plan = planHeal(
      triaged,
      JSON.parse(readFileSync(keywordsFile, "utf8")),
      thresholds
    )
    markdown = report(triaged, plan, { days: number("days", 7) })
  }
} finally {
  await server.close()
}

if (command === "demand") {
  const out = flag("out")
  if (out) {
    writeFileSync(`${out}.json`, `${JSON.stringify(items, null, 2)}\n`)
    writeFileSync(`${out}.md`, markdown)
  } else process.stdout.write(markdown)
  process.exit(0)
}

if (command === "triage" || rest.includes("--dry-run")) {
  const out = flag("out")
  if (out) writeFileSync(out, markdown)
  else process.stdout.write(markdown)
  process.exit(0)
}

const git = (...args) =>
  execFileSync("git", args, { cwd: office, encoding: "utf8" }).trim()
if (!plan.proposals.length) {
  console.log("Nothing to heal.")
  process.exit(0)
}
if (git("status", "--porcelain")) {
  console.error("Working tree is not clean; commit or set aside changes first.")
  process.exit(1)
}
const start = git("rev-parse", "--abbrev-ref", "HEAD")
const branch = `voice-heal/${new Date().toISOString().slice(0, 10)}`
git("switch", "-c", branch)
const files = [keywordsFile, healsFile]
try {
  writeFileSync(keywordsFile, JSON.stringify(plan.keywords, null, 2))
  const heals = JSON.parse(readFileSync(healsFile, "utf8"))
  const seen = new Set(heals.map((h) => `${h.intent}|${h.want}`))
  writeFileSync(
    healsFile,
    JSON.stringify(
      [
        ...heals,
        ...plan.fixture.filter((h) => !seen.has(`${h.intent}|${h.want}`)),
      ],
      null,
      2
    )
  )
  execFileSync("pnpm", ["exec", "prettier", "--write", ...files], {
    cwd: office,
    stdio: "ignore",
  })
  execFileSync("pnpm", ["exec", "vitest", "run", ...TESTS], {
    cwd: office,
    stdio: "inherit",
  })
  git("add", ...files)
  const lines = plan.proposals.map(
    (p) =>
      `- ${p.op} + ${p.words.join(", ")}${p.review ? " (human review)" : ""}`
  )
  git(
    "commit",
    "-m",
    `fix(office): voice trigger words from the gap ledger\n\n${lines.join("\n")}\n\nProposed by scripts/voice-heal.mjs; keywords, discovery replay and gap-heal tests green.`
  )
  git("switch", start)
  console.log(`Committed on ${branch} (not pushed):\n${lines.join("\n")}`)
  if (plan.proposals.some((p) => p.review))
    console.log("Contains proposals marked for human review.")
} catch (error) {
  // Red tests or a failed step: leave the developer exactly where they started.
  git("checkout", "--", ...files)
  git("switch", start)
  git("branch", "-D", branch)
  console.error(`Heal abandoned, nothing committed: ${error.message}`)
  process.exit(1)
}
