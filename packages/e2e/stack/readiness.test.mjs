import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { spawnSync } from "node:child_process"
import { test } from "node:test"
import { Script } from "node:vm"

import { nextBuildArgs } from "./up.mjs"

const workflow = readFileSync(
  new URL("../../../.github/workflows/ci.yml", import.meta.url),
  "utf8"
)

function job(name) {
  const start = workflow.indexOf("\n  " + name + ":")
  assert.ok(start >= 0, "job exists: " + name)
  const rest = workflow.slice(start + 1)
  // Only two-space job headings end a block; step/config headings have deeper indentation.
  return rest.split(/\n(?= {2}[a-z][a-z0-9-]*:\n)/)[0]
}

test("release tags require every aggregate and every candidate producer", () => {
  const images = job("images")
  const match = images.match(/ {4}if: >-\n([\s\S]*?)\n {4}needs: \[([^\]]+)\]/)
  assert.ok(
    match,
    "images has a reviewable predicate and explicit dependencies"
  )
  const dependencies = match[2].split(",").map((name) => name.trim())
  assert.deepEqual(dependencies, [
    "changes",
    "api",
    "web",
    "stack-e2e",
    "image-prepare",
  ])
  const condition = new Script("(" + match[1].trim() + ")")
  const context = {
    cancelled: () => false,
    github: { event_name: "push", ref: "refs/heads/main" },
    needs: Object.fromEntries(
      dependencies.map((name) => [name, { result: "success" }])
    ),
  }
  assert.equal(condition.runInNewContext(context), true)
  for (const dependency of dependencies)
    for (const result of ["failure", "cancelled", "skipped", ""]) {
      const needs = { ...context.needs, [dependency]: { result } }
      assert.equal(
        condition.runInNewContext({ ...context, needs }),
        false,
        dependency + "=" + result
      )
    }
  assert.equal(
    condition.runInNewContext({ ...context, cancelled: () => true }),
    false
  )
  assert.equal(
    condition.runInNewContext({
      ...context,
      github: { event_name: "pull_request", ref: "refs/heads/main" },
    }),
    false
  )
  assert.equal(
    condition.runInNewContext({
      ...context,
      github: { event_name: "push", ref: "refs/heads/other" },
    }),
    false
  )
})

test("aggregate checks accept only all-success children or legitimate unchanged skips", () => {
  for (const [name, children] of [
    ["api", ["api-quality", "api-tests"]],
    ["web", ["web-quality", "web-browser"]],
    ["stack-e2e", ["stack-tests"]],
  ]) {
    const block = job(name)
    assert.match(block, /!cancelled\(\) && needs.changes.result == 'success'/)
    for (const child of children)
      assert.ok(block.includes(child), "aggregate requires " + child)
    const script = block.match(/ {8}run: \|\n([\s\S]*)/)[1]
    for (const changed of ["true", "false"])
      for (const first of ["success", "skipped", "failure", "cancelled", ""])
        for (const second of children.length === 2
          ? ["success", "skipped", "failure", "cancelled", ""]
          : ["success"]) {
          const result = spawnSync("bash", ["-e", "-c", script], {
            env: { CHANGED: changed, RESULT_0: first, RESULT_1: second },
          })
          const required = changed === "true" ? "success" : "skipped"
          assert.equal(
            result.status === 0,
            first === required &&
              (children.length === 1 || second === required),
            name + ":" + [changed, first, second].join("/")
          )
        }
  }
})

test("preparation uses candidates and publishes its manifest only after scan and push", () => {
  const prepare = job("image-prepare")
  assert.match(prepare, /needs: changes/)
  assert.match(
    prepare,
    /if: github.event_name == 'push' && github.ref == 'refs\/heads\/main'/
  )
  assert.match(
    prepare,
    /GENESY_REF: ci-\$\{\{ github.run_id \}\}-\$\{\{ github.run_attempt \}\}/
  )
  assert.ok(
    prepare.indexOf("Scan (HIGH") <
      prepare.indexOf('images.py record "$PRODUCER"')
  )
  assert.ok(
    prepare.indexOf('images.py record "$PRODUCER"') <
      prepare.indexOf("Upload successful producer manifest")
  )
  assert.match(prepare, /if-no-files-found: error/)
  assert.match(job("images"), /actions: read/)
})

test("CI checks reviewed voice contracts and pinned checkpoint schema before builds", () => {
  assert.match(workflow, /run: pnpm --filter @genesy\/office voice:check/)
  assert.match(
    workflow,
    /run: pnpm --filter @genesy\/office exec node scripts\/check-voice-schema\.mjs/
  )
  assert.ok(
    workflow.indexOf("voice:check") <
      workflow.indexOf("pnpm turbo run typecheck lint test build")
  )
})

test("the stack Office build uses its package preparation while other build paths stay unchanged", () => {
  assert.deepEqual(nextBuildArgs("office"), [
    "--filter",
    "@genesy/office",
    "build",
  ])
  const office = JSON.parse(
    readFileSync(
      new URL("../../../apps/office/package.json", import.meta.url),
      "utf8"
    )
  )
  assert.ok(
    office.scripts.build.indexOf("prepare-voice-audio.mjs") <
      office.scripts.build.indexOf("next build")
  )
  // Flow runs its own package build too, so the voice audio step the parent adds is picked up.
  assert.deepEqual(nextBuildArgs("flow"), ["--filter", "@genesy/flow", "build"])
  for (const app of ["shop", "till", "landing"])
    assert.deepEqual(nextBuildArgs(app), [
      "--filter",
      `@genesy/${app}`,
      "exec",
      "next",
      "build",
    ])
})
