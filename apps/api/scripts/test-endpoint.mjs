// Build the selected integration module and any sibling modules it imports.
import { spawnSync } from "node:child_process"
import { createHash } from "node:crypto"
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"

const root = fileURLToPath(new URL("../", import.meta.url))
const api = join(root, "tests/api")
const modules = new Set(readdirSync(api)
  .filter(name => name !== "main.rs" && name.endsWith(".rs"))
  .map(name => name.slice(0, -3)))
const [selector, ...extra] = process.argv.slice(2)
const [feature, test] = (selector ?? "").split("::")

if (extra.length || !modules.has(feature) || !/^\w+(::\w+)*$/.test(selector)) {
  console.error("Usage: node scripts/test-endpoint.mjs <feature>[::<test_name>] (see tests/api/)")
  process.exit(2)
}

const selected = new Set([feature])
for (const name of selected) {
  const body = readFileSync(join(api, name + ".rs"), "utf8")
  // Include names in grouped and aliased imports, plus fully qualified references in expressions.
  const references = body.match(/\buse\s+[^;]+;|\b(?:crate|super)::\w+/g) ?? []
  for (const token of references.join(" ").match(/\w+/g) ?? []) {
    if (modules.has(token)) selected.add(token)
  }
}

// Immutable per selection. Concurrent callers can safely use different module lists.
const declarations = [...selected].sort().map(name =>
  `#[path = ${JSON.stringify(join(api, name + ".rs"))}] mod ${name};\n`
).join("")
const key = createHash("sha256").update(declarations).digest("hex")
const manifest = join(root, "target/endpoint-modules", key + ".rs")
mkdirSync(dirname(manifest), { recursive: true })
if (!existsSync(manifest)) {
  const temporary = manifest + "." + process.pid
  writeFileSync(temporary, declarations)
  renameSync(temporary, manifest)
}

const env = { SQLX_OFFLINE: "true", ...process.env, VERZO_ENDPOINT_MODULES: manifest }
const args = ["test", "--locked", "--test", "focus", "--"]
const filter = test ? [selector, "--exact"] : [feature + "::"]
// libtest succeeds on an empty filter. Check its inventory before reporting a passing test.
const list = spawnSync("cargo", [...args, ...filter, "--list"], {
  cwd: root, env, encoding: "utf8", stdio: ["inherit", "pipe", "inherit"],
})
if (list.error) throw list.error
if (list.status !== 0) process.exit(list.status ?? 1)
if (!list.stdout.split("\n").some(line => line.endsWith(": test") && line.startsWith(feature + "::"))) {
  console.error(`No endpoint tests matched ${selector}`)
  process.exit(1)
}
const run = spawnSync("cargo", [...args, ...filter], { cwd: root, env, stdio: "inherit" })
if (run.error) throw run.error
process.exit(run.status ?? 1)
