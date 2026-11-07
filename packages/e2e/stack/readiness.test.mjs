import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"
import { Script } from "node:vm"

import { nextBuildArgs } from "./up.mjs"

const workflow = readFileSync(new URL("../../../.github/workflows/ci.yml", import.meta.url), "utf8")

test("release images wait for stack e2e and block failed or cancelled checks", () => {
  const images = workflow.slice(workflow.indexOf("\n  images:"))
  const match = images.match(/ {4}if: >-\n([\s\S]*?)\n {4}needs: \[([^\]]+)\]/)
  assert.ok(match, "images has a reviewable predicate and explicit dependencies")
  assert.ok(match[2].split(",").map((job) => job.trim()).includes("stack-e2e"), "stack e2e must finish before releasing images")
  const condition = new Script(`(${match[1].trim()})`)
  const good = new Set(["success", "skipped"])
  for (const api of ["success", "skipped", "failure", "cancelled"])
    for (const web of ["success", "skipped", "failure", "cancelled"])
      for (const stack of ["success", "skipped", "failure", "cancelled"]) {
        const context = { cancelled: () => false, github: { event_name: "push", ref: "refs/heads/main" }, needs: { changes: { result: "success" }, api: { result: api }, web: { result: web }, "stack-e2e": { result: stack } } }
        assert.equal(condition.runInNewContext(context), good.has(api) && good.has(web) && good.has(stack), `${api}/${web}/${stack}`)
        assert.equal(condition.runInNewContext({ ...context, cancelled: () => true }), false)
        assert.equal(condition.runInNewContext({ ...context, github: { event_name: "pull_request", ref: "refs/heads/main" } }), false)
      }
})

test("CI checks reviewed voice contracts and pinned checkpoint schema before builds", () => {
  assert.match(workflow, /run: pnpm --filter @genesy\/office voice:check/)
  assert.match(workflow, /run: pnpm --filter @genesy\/office exec node scripts\/check-voice-schema\.mjs/)
  assert.ok(workflow.indexOf("voice:check") < workflow.indexOf("pnpm turbo run typecheck lint test build"))
})

test("the stack Office build uses its package preparation while other build paths stay unchanged", () => {
  assert.deepEqual(nextBuildArgs("office"), ["--filter", "@genesy/office", "build"])
  const office = JSON.parse(readFileSync(new URL("../../../apps/office/package.json", import.meta.url), "utf8"))
  assert.ok(office.scripts.build.indexOf("prepare-voice-audio.mjs") < office.scripts.build.indexOf("next build"))
  for (const app of ["shop", "till", "landing"])
    assert.deepEqual(nextBuildArgs(app), ["--filter", `@genesy/${app}`, "exec", "next", "build"])
})
