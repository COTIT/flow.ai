// Compare discovered tests, including every project, before accepting a sharded suite.
import assert from "node:assert/strict"
import { spawnSync } from "node:child_process"
import { fileURLToPath } from "node:url"
import process from "node:process"

export function inventory(report) {
  assert.equal(report.errors?.length ?? 0, 0, "Playwright discovery failed")
  const tests = []
  const visit = (suite, parents) => {
    const titles = [...parents, suite.title]
    for (const spec of suite.specs ?? [])
      for (const test of spec.tests)
        tests.push(
          JSON.stringify([
            test.projectName,
            spec.file,
            spec.line,
            spec.column,
            ...titles,
            spec.title,
            test.repeatEachIndex ?? 0,
          ])
        )
    for (const child of suite.suites ?? []) visit(child, titles)
  }
  for (const suite of report.suites) visit(suite, [])
  return tests.sort()
}

export function verify(whole, shards) {
  assert.ok(whole.length, "unsharded inventory is empty")
  assert.ok(
    shards.length && shards.every((shard) => shard.length),
    "a shard selects no tests"
  )
  const combined = shards.flat().sort()
  assert.equal(
    new Set(combined).size,
    combined.length,
    "shards select duplicate tests"
  )
  assert.deepEqual(
    combined,
    [...whole].sort(),
    "shards must cover the complete suite"
  )
}

export function discover(config, shard, run = spawnSync) {
  const args = [
    "exec",
    "playwright",
    "test",
    "--config",
    config,
    "--list",
    "--reporter=json",
  ]
  if (shard) args.push("--shard=" + shard)
  const result = run("pnpm", args, {
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  })
  assert.ifError(result.error)
  assert.equal(result.status, 0, result.stderr || "Playwright discovery failed")
  return inventory(JSON.parse(result.stdout))
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [config, countArg, ...extra] = process.argv.slice(2)
  const count = Number(countArg)
  assert.ok(
    config && Number.isInteger(count) && count > 0 && !extra.length,
    "usage: verify-shards.mjs <config> <count>"
  )
  const whole = discover(config)
  const shards = Array.from({ length: count }, (_, i) =>
    discover(config, String(i + 1) + "/" + count)
  )
  verify(whole, shards)
  console.log(
    "shards: " +
      whole.length +
      " tests covered exactly once across " +
      count +
      " shards (" +
      shards.map((s) => s.length).join("/") +
      ")"
  )
}
