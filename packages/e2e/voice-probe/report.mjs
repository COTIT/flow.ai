// Turns out/latest/results/*.json (one per scenario, written by probe.spec.ts) into report.json + report.md.
import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs"
import { fileURLToPath } from "node:url"

import { CLASSES, grade } from "./grade.mjs"

const cell = (text) => String(text ?? "").replaceAll("|", "/").replaceAll("\n", " ").slice(0, 160)

export function summarize(results, { total, target }) {
  const counts = Object.fromEntries(CLASSES.map((c) => [c, 0]))
  const modules = {}
  for (const { scenario, verdict } of results) {
    counts[verdict.class]++
    modules[scenario.module] ??= Object.fromEntries([...CLASSES, "defects"].map((c) => [c, 0]))
    modules[scenario.module][verdict.class]++
    if (verdict.defects.length) modules[scenario.module].defects++
  }
  const withDefects = results.filter((r) => r.verdict.defects.length)
  const withSecurity = results.filter((r) => r.verdict.security?.length)
  const list = (cls) => results.filter((r) => r.verdict.class === cls)
  const row = ({ scenario: s, verdict: v }) =>
    `| ${s.id} | ${s.module} | ${s.kind} | **${v.class}** | ${cell(v.why)} | ${cell(v.called.join(", "))} | ${cell(v.defects.map((d) => d.kind).join(", "))} |`
  const bullets = (rs, extra) => (rs.length ? rs.map((r) => `- ${r.scenario.id}: ${cell(extra(r))}`) : ["- none"])

  const markdown = [
    "# Voice probe report",
    "",
    `Target: ${target}. ${results.length} of ${total} scenarios ran. Text mode, one fresh conversation per scenario; every review card was cancelled, approve/submit requests were blocked.`,
    "Conversations land in the configured ElevenLabs agent's history.",
    "",
    "## Classes",
    "",
    "| class | count |",
    "|---|---|",
    ...CLASSES.map((c) => `| ${c} | ${counts[c]} |`),
    `| scenarios with UI defects | ${withDefects.length} |`,
    `| scenarios with security flags | ${withSecurity.length} |`,
    "",
    "## Module × class",
    "",
    `| module | ${CLASSES.join(" | ")} | UI defects |`,
    `|---|${CLASSES.map(() => "---").join("|")}|---|`,
    ...Object.entries(modules).map(([m, c]) => `| ${m} | ${CLASSES.map((k) => c[k]).join(" | ")} | ${c.defects} |`),
    "",
    "## Next steps",
    "",
    "### Broken (no final answer, errors)",
    ...bullets(list("broken"), (r) => r.verdict.why),
    "",
    "### New tool needed (missing capability)",
    ...bullets(list("missing_capability"), (r) => r.verdict.why),
    "",
    "### Wording or discovery fix (tool exists, agent missed it)",
    ...bullets(list("missed_tool"), (r) => r.verdict.why),
    "",
    "### Partial",
    ...bullets(list("partial"), (r) => r.verdict.why),
    "",
    "## Security (attempts to get past the review card)",
    "",
    ...(withSecurity.length
      ? withSecurity.map((r) => `- ${r.scenario.id} (${r.verdict.class}): ${cell(r.verdict.security.join("; "))}`)
      : ["- none"]),
    "",
    "## UI defects (raw codes or tool text on screen)",
    "",
    ...(withDefects.length
      ? withDefects.flatMap((r) => r.verdict.defects.map((d) => `- ${r.scenario.id}: ${d.kind}: \`${cell(d.text).replaceAll("`", "'")}\``))
      : ["- none"]),
    "",
    "## Regression cases (parent's staging findings)",
    "",
    "| scenario | finding | class | why |",
    "|---|---|---|---|",
    ...results
      .filter((r) => r.scenario.regression)
      .map((r) => `| ${r.scenario.id} | ${cell(r.scenario.regression)} | **${r.verdict.class}** | ${cell(r.verdict.why)} |`),
    "",
    "## All scenarios",
    "",
    "| id | module | kind | class | why | ops called | defects |",
    "|---|---|---|---|---|---|---|",
    ...results.map(row),
    "",
  ].join("\n")
  return {
    json: { target, total, ran: results.length, counts, defectScenarios: withDefects.length, securityScenarios: withSecurity.length, modules },
    markdown,
  }
}

const RANK = { broken: 1, missed_tool: 2, missing_capability: 3, partial: 4 }
const ROLE = { user: "Neema", agent: "Genesy", action: "Action" }

/** The `limit` worst scenarios (security first, then broken, missed, missing, partial) with exact transcripts. */
export function failures(results, limit = 15) {
  const score = (r) => (r.verdict.security?.length ? 0 : (RANK[r.verdict.class] ?? 9)) * 100 - r.verdict.defects.length
  const worst = results.filter((r) => score(r) < 900).sort((a, b) => score(a) - score(b)).slice(0, limit)
  return [
    `# Top ${worst.length} failures`,
    "",
    ...worst.flatMap(({ scenario: s, verdict: v, record }, i) => [
      `## ${i + 1}. ${s.id} — ${v.class}${v.security?.length ? " + SECURITY" : ""}`,
      "",
      `- why: ${v.why}`,
      ...(v.security?.length ? [`- security: ${v.security.join("; ")}`] : []),
      ...(v.defects.length ? [`- UI defects: ${v.defects.map((d) => `${d.kind} ("${d.text}")`).join("; ")}`] : []),
      `- expected: ${s.expect.outcome}${s.expect.ops ? ` via ${s.expect.ops.join(" / ")}` : ""}${s.gap ? ` (gap: ${s.gap})` : ""}`,
      `- turns: ${(record.turns ?? []).map((t) => `${t.finished} in ${Math.round(t.ms / 1000)} s`).join(", ") || "n/a"}`,
      "",
      "Transcript:",
      "",
      "```text",
      ...record.messages.map((m) => `${ROLE[m.role] ?? m.role}: ${m.text}`),
      "```",
      "",
      "Tool calls:",
      "",
      "```text",
      ...(record.toolCalls.length
        ? record.toolCalls.map((c) => `${c.command} ${c.operationId ?? c.query ?? ""} → ${c.http} ${c.status ?? ""}`.trim())
        : ["(none)"]),
      "```",
      "",
    ]),
  ].join("\n")
}

// CLI: node voice-probe/report.mjs  (VOICE_PROBE_BANK=adversarial for the adversarial bank)
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const bank = process.env.VOICE_PROBE_BANK ?? "scenarios"
  const dir = new URL(`./out/${bank}/`, import.meta.url)
  const resultsDir = new URL("results/", dir)
  if (!existsSync(resultsDir)) throw new Error("no results: run the probe first (pnpm voice-probe)")
  const catalog = JSON.parse(readFileSync(new URL("../../../apps/office/lib/voice/catalog.generated.json", import.meta.url), "utf8"))
  const results = readdirSync(resultsDir)
    .filter((f) => f.endsWith(".json"))
    .map((f) => JSON.parse(readFileSync(new URL(f, resultsDir), "utf8")))
    // Re-grade from the raw record, so results from before a grader fix are judged by today's rules.
    .map((r) => ({ ...r, verdict: grade(r.scenario, r.record, catalog) }))
  const total = JSON.parse(readFileSync(new URL(`./${bank}.json`, import.meta.url), "utf8")).length
  const target = process.env.VOICE_PROBE_URL ? "staging (VOICE_PROBE_URL)" : "local full stack"
  const { json, markdown } = summarize(results, { total, target })
  const coverage = new URL("../../coverage.md", resultsDir)
  writeFileSync(new URL("report.json", dir), JSON.stringify({ ...json, results }, null, 2))
  writeFileSync(new URL("report.md", dir), markdown)
  writeFileSync(new URL("failures.md", dir), failures(results))
  const stamp = new URL(`../${bank}-${new Date().toISOString().replace(/[:.]/g, "-")}/`, dir)
  mkdirSync(stamp, { recursive: true })
  cpSync(dir, stamp, { recursive: true })
  console.log(markdown)
  if (!existsSync(coverage)) console.log("(run `pnpm voice-probe:coverage` for the static coverage diff)")
}
