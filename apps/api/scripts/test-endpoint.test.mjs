// Exercise endpoint selection without compiling Rust or opening a database.
import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import test from "node:test"
import { fileURLToPath } from "node:url"

const runner = fileURLToPath(new URL("./test-endpoint.mjs", import.meta.url))

test("selects transitive helpers, rejects empty matches and propagates failures", t => {
  const dir = mkdtempSync(join(tmpdir(), "endpoint-runner-"))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const calls = join(dir, "calls")
  writeFileSync(join(dir, "cargo"), `#!/usr/bin/env node
const fs = require("node:fs")
fs.appendFileSync(process.env.CALLS, JSON.stringify({args: process.argv.slice(2), modules: fs.readFileSync(process.env.VERZO_ENDPOINT_MODULES, "utf8")}) + "\\n")
if (process.argv.includes("--list")) console.log(process.env.INVENTORY)
else process.exit(37)
`, { mode: 0o755 })

  function invoke(selector, inventory = "") {
    writeFileSync(calls, "")
    const result = spawnSync(process.execPath, [runner, selector], {
      env: { ...process.env, PATH: dir + ":" + process.env.PATH, CALLS: calls, INVENTORY: inventory },
    })
    return [result.status, readFileSync(calls, "utf8").split("\n").filter(Boolean).map(JSON.parse)]
  }

  const selector = "provider_observations::example"
  const [status, steps] = invoke(selector, selector + ": test")
  assert.equal(status, 37)
  assert.equal(steps.length, 2)
  assert.deepEqual(steps[1].args.slice(-3), ["--", selector, "--exact"])
  for (const name of ["provider_observations", "settlement", "accounting"]) {
    assert.ok(steps[1].modules.includes("mod " + name + ";"))
  }
  assert.ok(!steps[1].modules.includes("mod healthcheck;"))

  const [missing, onlyList] = invoke("healthcheck::missing")
  assert.equal(missing, 1)
  assert.equal(onlyList.length, 1)
  assert.match(onlyList[0].modules, /^#\[path = .*\] mod healthcheck;\n$/)
  assert.deepEqual(invoke("../outside"), [2, []])
})
