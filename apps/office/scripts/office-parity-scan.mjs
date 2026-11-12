// Which API operations the Office screens call, read from source: generated api-client imports
// (`@genesy/api-client/endpoints/*`, and the helper modules that wrap them) plus hand-built
// `apiRequest` URLs matched to operation-policy.json by method and path.
// Shared by lib/voice/office-parity.test.ts (G1/G5) and scripts/voice-coverage.mjs (G7).
import { readFileSync, readdirSync } from "node:fs"
import { join, relative } from "node:path"
import { fileURLToPath } from "node:url"

const office = fileURLToPath(new URL("..", import.meta.url))
const apiClient = join(office, "../../packages/api-client/src")
export const policy = JSON.parse(
  readFileSync(join(office, "lib/voice/operation-policy.json"), "utf8")
)

// Generated hooks are the camelCase operation id, maybe wrapped: useX, getXQueryKey, getXMutationOptions…
const byCamel = new Map(
  Object.keys(policy).map((id) => [
    id.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase()),
    id,
  ])
)
export function opOf(name) {
  const bare = name
    .replace(/^(use|get)(?=[A-Z])/, "")
    .replace(
      /(Infinite)?(QueryOptions|QueryKey|MutationOptions|MutationKey|Url)$/,
      ""
    )
  return byCamel.get(bare[0].toLowerCase() + bare.slice(1))
}

const IMPORT =
  /import\s+(?:type\s+)?\{([^}]*)\}\s+from\s+"(@genesy\/api-client\/[^"]+|\.\/generated\/[^"]+)"/g
const names = (list) =>
  list
    .split(",")
    .map((n) =>
      n
        .trim()
        .split(/\s+as\s+/)[0]
        .replace(/^type\s+/, "")
    )
    .filter(Boolean)

function importedOps(source) {
  const ops = new Set()
  const helpers = new Set()
  for (const [, list, from] of source.matchAll(IMPORT)) {
    if (
      from.startsWith("@genesy/api-client/endpoints/") ||
      from.startsWith("./generated/")
    )
      for (const n of names(list)) {
        const op = opOf(n)
        if (op) ops.add(op)
      }
    else helpers.add(from.slice("@genesy/api-client/".length))
  }
  return { ops, helpers }
}

const normal = (path) => path.replace(/\{[^}]*\}/g, "{}")
const byRoute = new Map(
  Object.entries(policy).map(([id, p]) => [`${p.method} ${normal(p.path)}`, id])
)

/** `apiRequest({ url, method })` calls in one file, resolved through the file's own `const x = \`…\``. */
function requestOps(source) {
  const consts = Object.fromEntries(
    [...source.matchAll(/const\s+(\w+)\s*=\s*`([^`]*)`/g)].map(([, k, v]) => [
      k,
      v,
    ])
  )
  const expand = (s, depth = 0) =>
    depth > 3
      ? s
      : s.replace(/\$\{(\w+)\}/g, (m, k) =>
          k in consts ? expand(consts[k], depth + 1) : m
        )
  const found = []
  for (const m of source.matchAll(/apiRequest\s*(?:<[^(]*?>)?\s*\(/g)) {
    const call = source.slice(m.index, m.index + 400)
    const url = call.match(/url:\s*(?:`([^`]*)`|(\w+))/)
    if (!url) continue
    const raw = url[1] ?? (url[2] in consts ? `\${${url[2]}}` : null)
    if (raw === null) {
      found.push({ op: undefined, route: `? ${url[2]}` })
      continue
    }
    const method = call.match(/method:\s*"(\w+)"/)?.[1] ?? "GET"
    const path = expand(raw)
      .split("?")[0]
      .replace(/\$\{[^}]*\}/g, "{}")
    const route = `${method} ${path}`
    found.push({ op: byRoute.get(route), route })
  }
  return found
}

function walk(dir, out = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      if (!["node_modules", ".next"].includes(entry.name)) walk(path, out)
    } else if (/\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name))
      out.push(path)
  }
  return out
}

/**
 * Every operation an Office screen calls → the files that call it.
 * Voice's own code (lib/voice, app/(app)/voice) is not an Office screen and is left out.
 * `unmatched` lists hand-built requests no policy route matches.
 */
export function officeScreenOps() {
  const files = ["app", "components", "lib"]
    .flatMap((d) => walk(join(office, d)))
    .filter(
      (f) =>
        !/[/\\]lib[/\\]voice[/\\]|[/\\]app[/\\]\(app\)[/\\]voice[/\\]|[/\\]components[/\\]voice[/\\]/.test(
          f
        )
    )
  const ops = new Map()
  const unmatched = []
  const helpers = new Set()
  const add = (op, where) => {
    if (!ops.has(op)) ops.set(op, new Set())
    ops.get(op).add(where)
  }
  for (const file of files) {
    const source = readFileSync(file, "utf8")
    const where = relative(office, file)
    const imported = importedOps(source)
    for (const op of imported.ops) add(op, where)
    for (const h of imported.helpers) helpers.add(h)
    for (const { op, route } of requestOps(source))
      if (op) add(op, where)
      else unmatched.push({ file: where, route })
  }
  // Helper modules (auth, rights, storefront-review…) call generated hooks themselves.
  for (const helper of helpers)
    for (const ext of [".ts", ".tsx"]) {
      let source
      try {
        source = readFileSync(join(apiClient, helper + ext), "utf8")
      } catch {
        continue
      }
      for (const op of importedOps(source).ops)
        add(op, `@genesy/api-client/${helper}`)
    }
  return { ops, unmatched }
}
