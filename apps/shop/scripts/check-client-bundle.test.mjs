import { spawnSync } from "node:child_process"
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { afterEach, describe, expect, it } from "vitest"

const script = fileURLToPath(
  new URL("./check-client-bundle.mjs", import.meta.url)
)
// Each case spawns a real node process; on a loaded build machine that alone can pass 5 s.
const SPAWN_TIMEOUT = 30_000

let app
afterEach(() => app && rmSync(app, { recursive: true, force: true }))

// App folder with a clean chunk in `.next` and `.next-stack`, plus any planted files.
function fixture(files = {}) {
  app = mkdtempSync(join(tmpdir(), "shop-bundle-"))
  const all = {
    ".next/static/chunks/main.js": 'fetch("/v1/catalog");',
    ".next-stack/static/chunks/main.js": 'fetch("/v1/catalog");',
    ...files,
  }
  for (const [file, text] of Object.entries(all)) {
    mkdirSync(dirname(join(app, file)), { recursive: true })
    writeFileSync(join(app, file), text)
  }
}

function check(env = {}) {
  const clean = Object.fromEntries(
    Object.entries(process.env).filter(
      ([k]) => k !== "NEXT_DIST_DIR" && k !== "STOREFRONT_APP_KEY"
    )
  )
  return spawnSync(process.execPath, [script, app], {
    encoding: "utf8",
    env: { ...clean, ...env },
  })
}

const leak = 'const k="STOREFRONT_APP_KEY";'

describe("shop check-client-bundle", () => {
  it(
    "passes a clean default .next",
    () => {
      fixture()
      const run = check()
      expect(run.status, run.stderr).toBe(0)
    },
    SPAWN_TIMEOUT
  )

  it(
    "fails on a leak in the default .next",
    () => {
      fixture({ ".next/static/chunks/leak.js": leak })
      const run = check()
      expect(run.status).toBe(1)
      expect(run.stderr).toContain(join(".next", "static", "chunks", "leak.js"))
    },
    SPAWN_TIMEOUT
  )

  it(
    "fails on a leak in the NEXT_DIST_DIR folder",
    () => {
      fixture({ ".next-stack/static/chunks/leak.js": leak })
      const run = check({ NEXT_DIST_DIR: ".next-stack" })
      expect(run.status).toBe(1)
      expect(run.stderr).toContain(
        join(".next-stack", "static", "chunks", "leak.js")
      )
    },
    SPAWN_TIMEOUT
  )

  it(
    "ignores .next when NEXT_DIST_DIR points elsewhere",
    () => {
      fixture({ ".next/static/chunks/leak.js": leak })
      const run = check({ NEXT_DIST_DIR: ".next-stack" })
      expect(run.status, run.stderr).toBe(0)
      expect(run.stdout).toContain(".next-stack")
    },
    SPAWN_TIMEOUT
  )
})
