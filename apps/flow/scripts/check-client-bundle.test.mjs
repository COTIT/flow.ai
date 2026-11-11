import { spawnSync } from "node:child_process"
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { afterEach, describe, expect, it } from "vitest"

const script = fileURLToPath(
  new URL("./check-client-bundle.mjs", import.meta.url)
)
// Each case spawns a real node process; under a loaded build machine that alone can pass 5 s.
const SPAWN_TIMEOUT = 30_000
const SECRETS = [
  "ASSISTANT_BRIDGE_SECRET",
  "ELEVENLABS_API_KEY",
  "LANGSMITH_API_KEY",
  "STOREFRONT_APP_KEY",
]

let app
afterEach(() => app && rmSync(app, { recursive: true, force: true }))

// A minimal clean build: client chunk, build manifest (rewrite source only), prerendered page.
function fixture(files = {}, dist = ".next") {
  app = mkdtempSync(join(tmpdir(), "flow-bundle-"))
  const all = {
    [`${dist}/static/chunks/main.js`]: 'fetch("/v1/me");',
    [`${dist}/static/abc123/_buildManifest.js`]:
      'self.__BUILD_MANIFEST={"__rewrites":{"afterFiles":[{"source":"/v1/:path*"}]}}',
    [`${dist}/server/app/login.html`]: "<html><body>Sign in</body></html>",
    ...files,
  }
  for (const [file, text] of Object.entries(all)) {
    mkdirSync(dirname(join(app, file)), { recursive: true })
    writeFileSync(join(app, file), text)
  }
  return app
}

function check(env = {}) {
  const clean = Object.fromEntries(
    Object.entries(process.env).filter(
      ([k]) => k !== "NEXT_DIST_DIR" && !SECRETS.includes(k)
    )
  )
  return spawnSync(process.execPath, [script, app], {
    encoding: "utf8",
    env: { ...clean, ...env },
  })
}

describe("check-client-bundle", () => {
  it(
    "passes a clean bundle",
    () => {
      fixture()
      const run = check()
      expect(run.status, run.stderr).toBe(0)
      expect(run.stdout).toContain("client bundle check: no server env names")
    },
    SPAWN_TIMEOUT
  )

  it.each([
    "VOICE_UPSTREAM_URL",
    "OFFICE_PUBLIC_URL",
    "API_URL",
    "ELEVENLABS_",
    "ASSISTANT_BRIDGE_SECRET",
    "ASSISTANT_CHECKPOINT_URL",
    "LANGSMITH_",
    "STOREFRONT_APP_KEY",
    "//office:3000",
    "//api:8080",
  ])(
    "fails on %s in a client chunk, naming file and pattern",
    (pattern) => {
      fixture({ ".next/static/chunks/leak.js": `const x="${pattern}";` })
      const run = check()
      expect(run.status).toBe(1)
      expect(run.stderr).toContain(`.next/static/chunks/leak.js: ${pattern}`)
    },
    SPAWN_TIMEOUT
  )

  it(
    "passes the public Office origin and localhost",
    () => {
      fixture({
        ".next/static/chunks/office.js": 'const o="https://office.genesy.app";',
        ".next/static/chunks/lib.js": 'new URL(p,"http://localhost")',
        ".next/server/app/index.html":
          '<a href="https://office.genesy.app/orders">Office</a>',
      })
      const run = check()
      expect(run.status, run.stderr).toBe(0)
    },
    SPAWN_TIMEOUT
  )

  it.each([
    ".next/server/app/page.html",
    ".next/server/app/page.rsc",
    ".next/server/app/icon.svg.body",
    ".next/server/app/page.segments/_full.segment.rsc",
    "public/voice-audio/worklet.js",
    ".next/static/chunks/main.js.map",
  ])(
    "scans browser-served %s",
    (file) => {
      fixture({ [file]: "VOICE_UPSTREAM_URL" })
      const run = check()
      expect(run.status).toBe(1)
      expect(run.stderr).toContain(`${file}: VOICE_UPSTREAM_URL`)
    },
    SPAWN_TIMEOUT
  )

  it(
    "ignores server-only build files",
    () => {
      fixture({
        ".next/server/chunks/route.js": "process.env.VOICE_UPSTREAM_URL",
        ".next/server/app/page_client-reference-manifest.js": "API_URL",
        ".next/server/app/page.meta": "API_URL",
        ".next/routes-manifest.json": '{"destination":"http://api:8080/v1"}',
        ".next/required-server-files.json": "API_URL",
        ".next/standalone/apps/flow/server.js": "API_URL",
      })
      const run = check()
      expect(run.status, run.stderr).toBe(0)
    },
    SPAWN_TIMEOUT
  )

  it(
    "reads NEXT_DIST_DIR",
    () => {
      fixture(
        { ".next-stack/static/chunks/leak.js": "//api:8080" },
        ".next-stack"
      )
      expect(check({ NEXT_DIST_DIR: ".next-stack" }).status).toBe(1)
      rmSync(app, { recursive: true, force: true })

      fixture(
        {
          ".next/static/chunks/main.js": "//api:8080",
          ".next-stack/static/chunks/main.js": "ok",
        },
        ".next-stack"
      )
      const run = check({ NEXT_DIST_DIR: ".next-stack" })
      expect(run.status, run.stderr).toBe(0)
      expect(run.stdout).toContain(".next-stack/static")
    },
    SPAWN_TIMEOUT
  )

  it(
    "fails when the build output is missing",
    () => {
      app = mkdtempSync(join(tmpdir(), "flow-bundle-"))
      const run = check()
      expect(run.status).toBe(1)
      expect(run.stderr).toContain("run next build first")
    },
    SPAWN_TIMEOUT
  )

  it.each(SECRETS)(
    "fails on the value of %s without printing it",
    (name) => {
      const value = "s3cr3t-value-0123456789"
      fixture({ ".next/static/chunks/inlined.js": `const k="${value}";` })
      const run = check({ [name]: value })
      expect(run.status).toBe(1)
      expect(run.stderr).toContain(`secret value of ${name}`)
      expect(run.stderr + run.stdout).not.toContain(value)
    },
    SPAWN_TIMEOUT
  )

  it(
    "fails on an app key shape",
    () => {
      fixture({ ".next/static/chunks/key.js": 'const k="gsk_abcdEFGH1234";' })
      const run = check()
      expect(run.status).toBe(1)
      expect(run.stderr).toContain("app key shape")
      expect(run.stderr).not.toContain("gsk_abcdEFGH1234")
    },
    SPAWN_TIMEOUT
  )

  it(
    "ignores short secret values",
    () => {
      fixture()
      const run = check({ ASSISTANT_BRIDGE_SECRET: "x" })
      expect(run.status, run.stderr).toBe(0)
    },
    SPAWN_TIMEOUT
  )
})
