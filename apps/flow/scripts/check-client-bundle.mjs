// Build check: no file a browser can download names a server-only env var, holds an internal
// compose address, or holds a secret value set in the build env. Runs after `next build`.
// Limit: a value split or encoded at runtime is not caught; `import "server-only"` in server
// modules (e.g. lib/voice-proxy.ts) stays the first line of defence.
import { existsSync, readdirSync, readFileSync } from "node:fs"
import { join, relative, resolve } from "node:path"
import process from "node:process"

// The optional argument exists for tests; the build runs it from the app folder with none.
const app = resolve(process.argv[2] ?? join(import.meta.dirname, ".."))
const dist = resolve(app, process.env.NEXT_DIST_DIR ?? ".next")
const static_ = join(dist, "static")

const names = [
  "VOICE_UPSTREAM_URL",
  "OFFICE_PUBLIC_URL",
  "SHOP_PUBLIC_URL", // F5
  "API_URL",
  "ELEVENLABS_",
  "ASSISTANT_BRIDGE_SECRET",
  "ASSISTANT_CHECKPOINT_URL",
  "LANGSMITH_",
  "STOREFRONT_APP_KEY",
  // Compose-internal upstreams (http, https or ws).
  "//office:3000",
  "//api:8080",
]
// URL values (OFFICE_PUBLIC_URL, SHOP_PUBLIC_URL, VOICE_UPSTREAM_URL, API_URL) are left out on purpose: the
// public Office origin belongs in pages, and local values are localhost origins.
const secrets = [
  "ASSISTANT_BRIDGE_SECRET",
  "ELEVENLABS_API_KEY",
  "LANGSMITH_API_KEY",
  "STOREFRONT_APP_KEY",
].filter((name) => (process.env[name] ?? "").length >= 8)
// App keys look like gsk_ followed by the secret (same as apps/shop's check).
const keyShape = /gsk_[A-Za-z0-9_-]{8,}/

if (!existsSync(static_)) {
  console.error(`no ${static_}: run next build first`)
  process.exit(1)
}

const files = (dir, keep = () => true) =>
  existsSync(dir)
    ? readdirSync(dir, { recursive: true, withFileTypes: true })
        .filter((f) => f.isFile() && keep(f.name))
        .map((f) => join(f.parentPath, f.name))
    : []

// Browser-served only: /_next/static, prerendered pages and RSC payloads, public/.
const served = [
  ...files(static_),
  ...files(join(dist, "server/app"), (n) => /\.(html|rsc|body)$/.test(n)),
  ...files(join(app, "public")),
]

const hits = served.flatMap((file) => {
  const text = readFileSync(file, "latin1")
  const at = relative(app, file)
  return [
    ...names.filter((n) => text.includes(n)).map((n) => `${at}: ${n}`),
    ...secrets
      .filter((s) => text.includes(process.env[s]))
      .map((s) => `${at}: secret value of ${s}`),
    ...(keyShape.test(text) ? [`${at}: app key shape gsk_…`] : []),
  ]
})

if (hits.length) {
  console.error(
    "Server-only value found in client files:\n" +
      hits.map((h) => `  ${h}`).join("\n")
  )
  process.exit(1)
}
console.log(
  `client bundle check: no server env names, internal addresses or secrets in ${relative(app, static_)}, ${relative(app, join(dist, "server/app"))}, public`
)
