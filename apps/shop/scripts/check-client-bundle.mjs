// Build check: no file the browser can download mentions the storefront app key.
// The key is a server secret (lib/storefront.ts is server-only); this catches a leak
// through a client import or a NEXT_PUBLIC_ copy. Runs after `next build`.
import { readdirSync, readFileSync } from "node:fs"
import { join } from "node:path"
import process from "node:process"

const dir = join(import.meta.dirname, "../.next/static")
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
console.log("client bundle check: no storefront key in .next/static")
