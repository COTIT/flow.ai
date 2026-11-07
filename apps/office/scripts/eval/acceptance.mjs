// Audit explicit capability evidence; missing rows never become passing coverage.
import { createHash } from "node:crypto"
import { readFile, readdir, writeFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"
import { resolve } from "node:path"

export const languages = ["en", "nl", "sw"]
const repetitions = [1, 2, 3]
const eligible = new Set(["execute", "secure_ui"])
const httpGates = new Set([
  "transport",
  "authorization",
  "validation",
  "result",
  "cross_workspace",
  "restricted_role",
  "farm_role",
  "four_eyes",
  "confirmation",
])
export const hash = (value) => createHash("sha256").update(value).digest("hex")

export function buildMatrix(operations, dataset) {
  const ids = new Set(operations.map((operation) => operation.id))
  if (ids.size !== operations.length)
    throw new Error("Duplicate catalog operation")
  const caseIds = new Set()
  for (const item of dataset.cases) {
    if (!ids.has(item.operationId) || !item.id || caseIds.has(item.id))
      throw new Error("Invalid or duplicate dataset case")
    caseIds.add(item.id)
  }
  return operations.map((operation) => {
    const required = []
    if (eligible.has(operation.disposition)) {
      required.push("transport", "authorization", "validation", "result")
      const auth = operation.authorization ?? {}
      if (
        auth.boundary === "workspace" ||
        operation.path.includes("{workspace}")
      )
        required.push("cross_workspace")
      if ([...(auth.allOf ?? []), ...(auth.anyOf ?? [])].length)
        required.push("restricted_role")
      if (
        (auth.roleAnyOf ?? []).length ||
        operation.domain?.startsWith("farm_")
      )
        required.push("farm_role")
      if (
        operation.id === "farm_records_households_verify" ||
        (auth.predicates ?? []).some((value) =>
          /four.?eyes|different.*(?:officer|user|verif)|not.*(?:creator|register|verif)/i.test(
            value
          )
        )
      )
        required.push("four_eyes")
      if ((operation.effects ?? []).length) required.push("confirmation")
      required.push(
        operation.disposition === "execute" ? "bridge" : "secure_handoff"
      )
    }
    return {
      operationId: operation.id,
      domain: operation.domain ?? operation.id.split("_")[0],
      disposition: operation.disposition,
      availability: operation.availability ?? "available",
      unavailableReason: operation.unavailableReason ?? null,
      method: operation.method,
      path: operation.path,
      required,
      cases: dataset.cases.filter((item) => item.operationId === operation.id),
    }
  })
}

function validKind(gate, kind) {
  if (httpGates.has(gate)) return kind === "real_http" || kind === "real_ui"
  if (gate === "secure_handoff") return kind === "real_ui"
  return kind === "real_native"
}

export function summarize(
  matrix,
  evidence,
  { catalogHash, datasetHash, model = null }
) {
  const models = [
    ...new Set(
      evidence
        .filter((record) => record.kind === "real_native")
        .map((record) => record.model)
        .filter(Boolean)
    ),
  ]
  if (!model && models.length > 1)
    throw new Error(
      "Select one model; evidence from different models cannot be combined"
    )
  model ??= models[0] ?? null
  const native = evidence.filter(
    (record) => record.kind === "real_native" && record.model === model
  )
  for (const field of ["agentVersion", "graphVersion"]) {
    if (
      native.some(
        (record) => typeof record[field] !== "string" || !record[field]
      ) ||
      new Set(native.map((record) => record[field])).size > 1
    )
      throw new Error(
        "Native evidence requires one immutable agent and graph version"
      )
  }
  for (const record of evidence) {
    const row = matrix.find((item) => item.operationId === record.operationId)
    const item = row?.cases.find((item) => item.id === record.caseId)
    if (
      record.catalogHash !== catalogHash ||
      record.datasetHash !== datasetHash ||
      !item ||
      !row.required.length ||
      !record.runId ||
      typeof record.passed !== "boolean" ||
      ![...row.required, "language"].includes(record.gate)
    )
      throw new Error("Invalid, stale, or unregistered evidence")
    if (
      record.gate === "language" &&
      (!languages.includes(record.language) ||
        !repetitions.includes(record.repetition) ||
        !item.commands?.[record.language])
    )
      throw new Error("Invalid language evidence")
    if (record.kind === "real_native" && !record.model)
      throw new Error("Native evidence requires exact model identity")
    if (
      record.kind === "real_native" &&
      (record.modelSource !== "transcript.producing_llm" ||
        record.agentVersionSource !== "conversation.version_id")
    )
      throw new Error("Native evidence requires verified actual identity sources")
  }
  const operations = matrix.map((row) => {
    const records = evidence.filter(
      (record) =>
        record.operationId === row.operationId &&
        (record.kind !== "real_native" || record.model === model)
    )
    const gates = Object.fromEntries(
      row.required.map((gate) => {
        const results =
          gate === "result" && row.availability === "unavailable"
            ? []
            : records.filter(
                (record) => record.gate === gate && validKind(gate, record.kind)
              )
        return [
          gate,
          {
            status: results.some((record) => !record.passed)
              ? "failed"
              : results.length
                ? "passed"
                : "missing",
            caseIds: [...new Set(results.map((record) => record.caseId))],
          },
        ]
      })
    )
    const canonical = row.cases.find((item) =>
      languages.every(
        (language) =>
          typeof item.commands?.[language] === "string" &&
          item.commands[language].trim()
      )
    )
    const languageResults = Object.fromEntries(
      languages.map((language) => {
        const results = repetitions.flatMap((repetition) => {
          const attempts = records.filter(
            (record) =>
              record.caseId === canonical?.id &&
              record.gate === "language" &&
              record.kind === "real_native" &&
              record.language === language &&
              record.repetition === repetition
          )
          return attempts.length
            ? [{ passed: attempts.every((record) => record.passed) }]
            : []
        })
        return [
          language,
          {
            runs: results.length,
            successes: results.filter((item) => item.passed).length,
            status:
              results.length < 3
                ? "missing"
                : results.every((item) => !item.passed)
                  ? "failed"
                  : "complete",
          },
        ]
      })
    )
    return {
      ...row,
      cases: row.cases.map((item) => item.id),
      canonicalStatus: canonical ? "ready" : "missing",
      gates,
      languages: languageResults,
    }
  })
  const included = operations.filter((row) => row.required.length)
  const unavailableCapabilities = included
    .filter((row) => row.availability === "unavailable")
    .map((row) => ({
      operationId: row.operationId,
      reason: row.unavailableReason,
    }))
  const languageTotals = Object.fromEntries(
    languages.map((language) => {
      const runs = included.reduce(
        (sum, row) => sum + row.languages[language].runs,
        0
      )
      const successes = included.reduce(
        (sum, row) => sum + row.languages[language].successes,
        0
      )
      return [
        language,
        {
          requiredRuns: included.length * 3,
          runs,
          successes,
          successRate: runs ? successes / runs : null,
        },
      ]
    })
  )
  const gateReady = included.every((row) =>
    Object.values(row.gates).every((gate) => gate.status === "passed")
  )
  const languageReady = languages.every(
    (language) =>
      languageTotals[language].runs === languageTotals[language].requiredRuns &&
      languageTotals[language].successRate >= 0.95 &&
      included.every((row) => row.languages[language].successes > 0)
  )
  const measuredRuns = new Map()
  for (const record of evidence.filter(
    (record) => record.kind === "real_native" && record.model === model
  )) {
    const key = JSON.stringify([
      record.runId,
      record.operationId,
      record.caseId,
      record.language,
      record.repetition,
    ])
    const prior = measuredRuns.get(key)
    if (
      prior &&
      Number.isInteger(prior.costUsdMicros) &&
      Number.isInteger(record.costUsdMicros) &&
      prior.costUsdMicros !== record.costUsdMicros
    )
      throw new Error("Conflicting cost evidence for one native run")
    if (!prior || Number.isInteger(record.costUsdMicros))
      measuredRuns.set(key, record)
  }
  const runs = [...measuredRuns.values()]
  const costs = runs.filter(
    (record) =>
      Number.isSafeInteger(record.costUsdMicros) &&
      record.costUsdMicros >= 0 &&
      record.costSource === "provider_metadata_verified_units" &&
      record.usageSnapshot === "post_end_observed" &&
      record.conversationStatus === "done"
  )
  const latencies = runs
    .map((record) => record.latencyMs)
    .filter((value) => Number.isFinite(value) && value >= 0)
    .sort((a, b) => a - b)
  const measurements = {
    costBasis: "observed_post_end",
    billingFinality: "unverified",
    runs: runs.length,
    costMeasuredRuns: costs.length,
    costUsdMicros: costs.length
      ? costs.reduce((sum, record) => sum + record.costUsdMicros, 0)
      : null,
    latencyMeasuredRuns: latencies.length,
    latencyP50Ms: latencies.length
      ? latencies[Math.ceil(latencies.length * 0.5) - 1]
      : null,
    latencyP95Ms: latencies.length
      ? latencies[Math.ceil(latencies.length * 0.95) - 1]
      : null,
  }
  return {
    formatVersion: 1,
    scope: "capability_acceptance_only",
    releaseApproval: "not_assessed",
    catalogHash,
    datasetHash,
    model,
    agentVersion: native[0]?.agentVersion ?? null,
    graphVersion: native[0]?.graphVersion ?? null,
    total: operations.length,
    eligible: included.length,
    unavailableCapabilities,
    releaseReady:
      included.length > 0 &&
      !unavailableCapabilities.length &&
      gateReady &&
      languageReady,
    costEvidenceComplete:
      runs.length > 0 &&
      costs.length === runs.length &&
      latencies.length === runs.length,
    measurements,
    languages: languageTotals,
    operations,
  }
}

export function estimateRun({
  capabilities,
  languages,
  repetitions,
  dailyLimit,
  concurrency,
  costPerRun = null,
}) {
  if (
    ![capabilities, languages, repetitions, dailyLimit, concurrency].every(
      (value) => Number.isInteger(value) && value > 0
    )
  )
    throw new Error("Run dimensions and limits must be positive integers")
  if (costPerRun !== null && (!Number.isFinite(costPerRun) || costPerRun < 0))
    throw new Error("Cost must be an observed nonnegative price")
  const runs = capabilities * languages * repetitions
  return {
    runs,
    dailyLimit,
    concurrency,
    minimumDays: Math.ceil(runs / dailyLimit),
    estimatedCost: costPerRun === null ? null : runs * costPerRun,
    note: "Daily limit may also be consumed by other sessions; no provider limits changed.",
  }
}

export function safeLocalURL(value) {
  const url = new URL(value)
  if (
    url.protocol !== "http:" ||
    !["127.0.0.1", "[::1]"].includes(url.hostname) ||
    url.username ||
    url.password ||
    url.search ||
    url.hash
  )
    throw new Error(
      "Use a literal loopback HTTP URL without credentials, query or fragment"
    )
  return url
}

export function evaluateAssertions(body, assertions) {
  return (
    assertions.length > 0 &&
    assertions.every((assertion) => {
      const value = assertion.path
        .split(".")
        .reduce((value, key) => value?.[key], body)
      if (
        assertion.type &&
        (assertion.type === "array"
          ? !Array.isArray(value)
          : assertion.type === "object"
            ? value === null ||
              Array.isArray(value) ||
              typeof value !== "object"
            : typeof value !== assertion.type)
      )
        return false
      if (
        "equals" in assertion &&
        JSON.stringify(value) !== JSON.stringify(assertion.equals)
      )
        return false
      if (
        "maxLength" in assertion &&
        (!Array.isArray(value) || value.length > assertion.maxLength)
      )
        return false
      return value !== undefined
    })
  )
}

export function mergeReviews(bundles) {
  const result = { reviews: [], sources: {} }
  const ids = new Set()
  for (const bundle of bundles) {
    for (const review of bundle.reviews) {
      if (ids.has(review.id)) throw new Error("Duplicate review ID")
      ids.add(review.id)
      result.reviews.push(review)
    }
    for (const [file, hash] of Object.entries(bundle.sources ?? {})) {
      if (result.sources[file] && result.sources[file] !== hash)
        throw new Error("Conflicting review source hashes")
      result.sources[file] = hash
    }
  }
  return result
}

export async function loadInputs() {
  const directory = new URL("./", import.meta.url)
  const catalogRaw = await readFile(
    new URL("../../lib/voice/catalog.generated.json", directory),
    "utf8"
  )
  const datasetRaw = await readFile(new URL("fixtures.json", directory), "utf8")
  const reviewFiles = (await readdir(directory))
    .filter((file) => /^rust-reviews(?:\.[a-z-]+)?\.json$/.test(file))
    .sort()
  const reviewInputs = await Promise.all(
    reviewFiles.map(async (name) => ({
      name,
      raw: await readFile(new URL(name, directory), "utf8"),
    }))
  )
  const catalog = JSON.parse(catalogRaw)
  const dataset = JSON.parse(datasetRaw)
  const reviews = mergeReviews(
    reviewInputs.map((input) => JSON.parse(input.raw))
  )
  dataset.cases.push(
    ...catalog
      .filter((operation) => eligible.has(operation.disposition))
      .map((operation) => ({
        id: `rust-http:${operation.id}`,
        operationId: operation.id,
        scenario: "executed_rust_transport",
      }))
  )
  for (const review of reviews.reviews)
    for (const operationId of review.operations)
      dataset.cases.push({
        id: `rust-review:${review.id}:${operationId}`,
        operationId,
        scenario: "reviewed_rust_assertions",
      })
  return {
    catalog,
    dataset,
    reviews,
    identity: {
      catalogHash: hash(catalogRaw),
      datasetHash: hash(
        datasetRaw +
          "\n" +
          reviewInputs.map((input) => input.name + "\n" + input.raw).join("\n")
      ),
    },
  }
}

async function main() {
  const { catalog, dataset, identity } = await loadInputs()
  const matrix = buildMatrix(catalog, dataset)
  const command = process.argv[2] ?? "inventory"
  const option = (name) => {
    const index = process.argv.indexOf(`--${name}`)
    return index < 0 ? undefined : process.argv[index + 1]
  }
  let result
  if (command === "inventory") result = summarize(matrix, [], identity)
  else if (command === "budget")
    result = estimateRun({
      capabilities: matrix.filter((row) => row.required.length).length,
      languages: 3,
      repetitions: 3,
      dailyLimit: Number(option("daily-limit") ?? 50),
      concurrency: Number(option("concurrency") ?? 2),
    })
  else if (command === "report") {
    if (!option("evidence")) throw new Error("Specify --evidence JSONL file")
    const records = (await readFile(option("evidence"), "utf8"))
      .split("\n")
      .filter(Boolean)
      .map((line) => JSON.parse(line))
    result = summarize(matrix, records, { ...identity, model: option("model") })
    if (!result.releaseReady) process.exitCode = 1
  } else throw new Error("Choose inventory, budget or report")
  const output = `${JSON.stringify(result, null, 2)}\n`
  if (option("output"))
    await writeFile(option("output"), output, { mode: 0o600 })
  else process.stdout.write(output)
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main().catch(() => {
    process.stderr.write(
      "Acceptance command failed: invalid arguments, dataset, or evidence. No credentials or response data were printed.\n"
    )
    process.exitCode = 1
  })
