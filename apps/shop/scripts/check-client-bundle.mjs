// Build check: no file the browser can download mentions the storefront app key.
// The key is a server secret (lib/storefront.ts is server-only); this catches a leak
// through a client import or a NEXT_PUBLIC_ copy. Runs after `next build`.
import { readdirSync, readFileSync } from "node:fs"
import { join, relative, resolve } from "node:path"
import process from "node:process"

// The optional argument exists for tests; the build runs it from the app folder with none.
const app = resolve(process.argv[2] ?? join(import.meta.dirname, ".."))
// Same rule as next.config.ts distDir: the e2e stack builds to its own folder.
const dir = join(resolve(app, process.env.NEXT_DIST_DIR ?? ".next"), "static")
const key = process.env.STOREFRONT_APP_KEY
const needles = ["STOREFRONT_APP_KEY", ...(key ? [key] : [])]
// App keys look like gsk_ followed by the secret.
const keyShape = /gsk_[A-Za-z0-9_-]{8,}/

const leaks = readdirSync(dir, { recursive: true, withFileTypes: true })
  .filter((f) => f.isFile())
  .map((f) => join(f.parentPath, f.name))
  .filter((file) => {
    const text = readFileSync(file, "latin1")
    return needles.some((n) => text.includes(n)) || keyShape.test(text)
  })

if (leaks.length) {
  console.error(
    "Storefront app key found in client files:\n" + leaks.join("\n")
  )
  process.exit(1)
}
console.log(`client bundle check: no storefront key in ${relative(app, dir)}`)
