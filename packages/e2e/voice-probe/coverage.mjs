// Static coverage: which API operations Office screens use that the voice assistant cannot reach,
// and a module × verb grid of what voice can reach. Offline: reads repo files only.
import { readFileSync, readdirSync, statSync, writeFileSync, mkdirSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

export const VERBS = ["find", "list", "create", "update", "cancel", "explain", "compare", "export", "undo"]

const MODULES = {
  orders: ["orders", "sales_groups", "shipment_methods"],
  products: ["products", "product_", "pricing", "price_corrections", "special_offers", "discount", "vat_groups"],
  stock: ["stock", "lots", "warehouse", "storage_locations"],
  supply: ["farm_", "supply_"],
  invoices: ["invoice"],
  till: ["till"],
  crm: ["relations", "contacts", "deal"],
  search: ["search"],
}

/** Neema's module for a catalog domain; anything unlisted is admin (keys, webhooks, team, settings…). */
export function moduleOf(domain) {
  for (const [module, prefixes] of Object.entries(MODULES))
    if (prefixes.some((p) => domain === p || domain.startsWith(p))) return module
  return "admin"
}

// First match wins; order matters (e.g. "mark_as_credited" is undo before "create").
const VERB_RULES = [
  ["undo", /_(refund|returns|credit|mark_as_credited)$/],
  ["export", /_(preview|export)$/],
  ["compare", /_(sales_summary|sale_statics|production|redemption_totals|data_quality|verify_summary|results)$/],
  ["explain", /_(events|history|timeline|reconciliation|holds|financial_summary|logs|movements|variables|available_fields|eligibility|estimate)$/],
  ["cancel", /_(cancel|void|revoke|delete|delete_many|delete_all|delete_status|remove_users|remove_question|detach|unpublish|withdraw_consent)$/],
  ["create", /_(create|create_all|create_status|register|grant|generate|duplicate|invoice|issue|send|resend|add_users|add_question|add_relation|attach|give_consent|answer|capacity_create)$/],
  ["find", /_(get|get_all|search|quick|lookup|passport_get|me|mine)$/],
  ["list", /_(list|list_all|column|by_warehouse|negative|stock|options|dispatch_options|catalog|relations|linked_products|images_list|configured_bins|with_bins|farmer_credits|matches|village_matches|village_supply|village_energy|village_pipeline|harvest_available|all|holdings|vouchers|redemptions|person|send_tasks)$/],
]

/** Verb for an operation id; whatever is left (set, move, publish, confirm…) changes a record: update. */
export function verbOf(id) {
  for (const [verb, re] of VERB_RULES) if (re.test(id)) return verb
  return "update"
}

const snake = (camel) => camel.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)

/** Operation ids Office imports from `@genesy/api-client/endpoints/*` (orval names: opId camelCased). */
export function officeOperations(sources) {
  const ops = new Set()
  const block = /import\s*\{([^}]*)\}\s*from\s*"@genesy\/api-client\/endpoints\/[^"]+"/g
  for (const src of sources)
    for (const [, names] of src.matchAll(block))
      for (const raw of names.split(",")) {
        const name = raw.trim()
        if (!name || name.startsWith("type ") || /^[A-Z]/.test(name)) continue
        const base = name
          .replace(/^use(?=[A-Z])/, "")
          .replace(/^get([A-Z]\w*?)(QueryKey|QueryOptions|MutationKey|MutationOptions|Url)$/, "$1")
        ops.add(snake(base[0].toLowerCase() + base.slice(1)))
      }
  return ops
}

const reachable = (op) => op.disposition === "execute" || op.disposition === "secure_ui"

export function coverage(catalog, used) {
  const byId = new Map(catalog.map((op) => [op.id, op]))
  const unreachable = [...used]
    .filter((id) => !byId.has(id) || !reachable(byId.get(id)))
    .sort()
    .map((id) => {
      const op = byId.get(id)
      return { id, why: op ? `${op.disposition}: ${op.reason ?? ""}`.trim() : "not in voice catalog" }
    })
  const grid = {}
  for (const module of [...Object.keys(MODULES), "admin"]) grid[module] = Object.fromEntries(VERBS.map((v) => [v, []]))
  for (const op of catalog.filter(reachable)) grid[moduleOf(op.domain)][verbOf(op.id)].push(op.id)

  const lines = [
    "# Voice static coverage",
    "",
    `Office uses ${used.size} API operations; ${unreachable.length} are not reachable by voice.`,
    "",
    "## Office operations voice cannot reach",
    "",
    "| operation | why |",
    "|---|---|",
    ...unreachable.map((u) => `| ${u.id} | ${u.why.replaceAll("|", "/")} |`),
    "",
    "## Reachable voice operations, module × verb (count)",
    "",
    `| module | ${VERBS.join(" | ")} |`,
    `|---|${VERBS.map(() => "---").join("|")}|`,
    ...Object.entries(grid).map(([m, row]) => `| ${m} | ${VERBS.map((v) => row[v].length).join(" | ")} |`),
    "",
    "Empty cells are verbs voice cannot do in that module at all.",
    "",
  ]
  return { unreachable, grid, markdown: lines.join("\n") }
}

function sourcesUnder(dir) {
  const out = []
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name.startsWith(".next")) continue
    const path = join(dir, name)
    if (statSync(path).isDirectory()) out.push(...sourcesUnder(path))
    else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)) out.push(readFileSync(path, "utf8"))
  }
  return out
}

// CLI: node voice-probe/coverage.mjs [outDir]
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const repo = join(dirname(fileURLToPath(import.meta.url)), "../../..")
  const office = join(repo, "apps/office")
  const catalog = JSON.parse(readFileSync(join(office, "lib/voice/catalog.generated.json"), "utf8"))
  // shortcut: only generated-client imports count; the few hand-written fetch paths in Office are skipped.
  const used = officeOperations(["app", "components", "lib"].flatMap((d) => sourcesUnder(join(office, d))))
  const report = coverage(catalog, used)
  const out = process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), "out")
  mkdirSync(out, { recursive: true })
  writeFileSync(join(out, "coverage.json"), JSON.stringify({ used: [...used].sort(), ...report, markdown: undefined }, null, 2))
  writeFileSync(join(out, "coverage.md"), report.markdown)
  console.log(report.markdown)
}
