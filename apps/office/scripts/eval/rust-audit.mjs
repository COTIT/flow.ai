// Join real HTTP observations to passed Rust tests; result assertions still require review.
import { readFile, writeFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"
import { resolve } from "node:path"
import { hash, loadInputs } from "./acceptance.mjs"

export function parseTestResults(log) {
  const results = new Map()
  for (const match of log.matchAll(
    /^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored)(?:[ \t].*)?$/gm
  )) {
    if (results.get(match[1]) !== "FAILED") results.set(match[1], match[2])
  }
  return results
}

export function auditHTTP(catalog, observations, results, catalogHash = null) {
  for (const observation of observations) {
    const operation = catalog.find(
      (operation) => operation.id === observation.operationId
    )
    if (
      !operation ||
      operation.method !== observation.method ||
      observation.contractValidated !== true ||
      !Number.isInteger(observation.status) ||
      observation.status < 100 ||
      observation.status > 599 ||
      typeof observation.test !== "string" ||
      (catalogHash &&
        observation.catalogHash &&
        observation.catalogHash !== catalogHash)
    )
      throw new Error("Invalid or stale HTTP observation")
  }
  const operations = catalog.map((operation) => {
    const requests = observations.filter(
      (request) => request.operationId === operation.id
    )
    const passed = requests.filter(
      (request) => results.get(request.test) === "ok"
    )
    const group = (items) =>
      [...new Set(items.map((item) => `${item.test}:${item.status}`))].map(
        (key) => {
          const index = key.lastIndexOf(":")
          return {
            test: key.slice(0, index),
            status: Number(key.slice(index + 1)),
          }
        }
      )
    return {
      operationId: operation.id,
      domain: operation.domain,
      disposition: operation.disposition,
      status: passed.length ? "executed_candidate" : "missing",
      passedTestObservations: group(passed),
      unprovenObservations: group(
        requests.filter((request) => results.get(request.test) !== "ok")
      ),
      assertionReview: "missing",
    }
  })
  const included = operations.filter((operation) =>
    ["execute", "secure_ui"].includes(operation.disposition)
  )
  return {
    formatVersion: 1,
    catalogHash,
    eligible: included.length,
    observedInPassedTests: included.filter(
      (operation) => operation.status === "executed_candidate"
    ).length,
    formalResultPasses: 0,
    note: "Executed request + passed test is candidate evidence. Review operation-specific assertions before assigning result, authorization, tenant, role or four-eyes gates.",
    operations,
  }
}

export function matchReviews(
  catalog,
  observations,
  results,
  reviews,
  verifiedSources
) {
  const records = []
  for (const review of reviews) {
    if (
      !review.assertions.length ||
      review.assertions.some(
        (assertion) => !verifiedSources.has(assertion.file)
      ) ||
      results.get(review.test) !== "ok"
    )
      continue
    for (const operationId of review.operations) {
      const operation = catalog.find(
        (operation) => operation.id === operationId
      )
      if (!operation) throw new Error("Unknown reviewed operation")
      for (const [gate, statuses] of Object.entries(review.gates)) {
        if (gate === "result" && operation.availability === "unavailable")
          continue
        const matches = observations.filter(
          (observation) =>
            observation.operationId === operationId &&
            observation.test === review.test &&
            statuses.includes(observation.status)
        )
        if (matches.length)
          records.push({
            operationId,
            caseId: `rust-review:${review.id}:${operationId}`,
            gate,
            kind: "real_http",
            passed: true,
            test: review.test,
            observedStatuses: [
              ...new Set(matches.map((match) => match.status)),
            ],
          })
      }
    }
  }
  return records
}

async function main() {
  const option = (name) => {
    const index = process.argv.indexOf(`--${name}`)
    return index < 0 ? undefined : process.argv[index + 1]
  }
  if (!option("observations") || !option("test-log") || !option("output"))
    throw new Error("Specify observations, test-log and output")
  const { catalog, reviews, identity } = await loadInputs()
  const callsRaw = await readFile(option("observations"), "utf8")
  const logRaw = await readFile(option("test-log"), "utf8")
  const observations = callsRaw
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
  const tests = parseTestResults(logRaw)
  const result = auditHTTP(catalog, observations, tests, identity.catalogHash)
  result.observationsHash = hash(callsRaw)
  result.testLogHash = hash(logRaw)
  const sources = new Set()
  const root = new URL("../../../../", import.meta.url)
  for (const [file, expected] of Object.entries(reviews.sources ?? {})) {
    if (!/^apps\/api\/tests\/[a-zA-Z0-9_/]+\.rs$/.test(file))
      throw new Error("Unexpected review source")
    const source = await readFile(new URL(file, root), "utf8")
    if (
      hash(source) !== expected ||
      reviews.reviews
        .flatMap((review) => review.assertions)
        .filter((assertion) => assertion.file === file)
        .some((assertion) => !source.includes(assertion.contains))
    )
      throw new Error("Stale assertion source review")
    sources.add(file)
  }
  const matched = matchReviews(
    catalog,
    observations,
    tests,
    reviews.reviews,
    sources
  )
  const runId = hash(callsRaw + logRaw)
  const transport = result.operations
    .filter(
      (operation) =>
        ["execute", "secure_ui"].includes(operation.disposition) &&
        operation.passedTestObservations.some(
          (observation) => observation.status >= 200 && observation.status < 300
        )
    )
    .map((operation) => ({
      operationId: operation.operationId,
      caseId: `rust-http:${operation.operationId}`,
      gate: "transport",
      kind: "real_http",
      passed: true,
    }))
  const evidence = [...transport, ...matched].map((record) => ({
    ...identity,
    runId,
    ...record,
  }))
  result.formalResultPasses = new Set(
    matched
      .filter((record) => record.gate === "result")
      .map((record) => record.operationId)
  ).size
  result.reviewedGateRecords = matched.length
  if (option("evidence-output"))
    await writeFile(
      option("evidence-output"),
      evidence.map((record) => JSON.stringify(record)).join("\n") + "\n",
      { mode: 0o600 }
    )
  await writeFile(option("output"), JSON.stringify(result, null, 2) + "\n", {
    mode: 0o600,
  })
  process.stdout.write(
    JSON.stringify({
      eligible: result.eligible,
      observedInPassedTests: result.observedInPassedTests,
      missing: result.eligible - result.observedInPassedTests,
      formalResultPasses: result.formalResultPasses,
      reviewedGateRecords: matched.length,
    }) + "\n"
  )
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main().catch(() => {
    process.stderr.write(
      "Rust HTTP audit failed: invalid, stale or missing observation/test artifacts.\n"
    )
    process.exitCode = 1
  })
