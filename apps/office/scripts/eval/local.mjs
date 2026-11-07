// Small real-HTTP read pilot. Never dispatch effects or call provider APIs.
import { randomUUID } from "node:crypto"
import { readFile, writeFile } from "node:fs/promises"
import { fileURLToPath } from "node:url"
import { resolve } from "node:path"
import { evaluateAssertions, loadInputs, safeLocalURL } from "./acceptance.mjs"

export function prepareRead(operation, fixture, auth) {
  if (
    operation.method !== "GET" ||
    operation.disposition !== "execute" ||
    operation.effects.length
  )
    throw new Error("Pilot accepts read-only executable operations")
  const allowed = ["workspaceId", "productId", "currencyId", "warehouseId"]
  const bind = (value) => {
    if (typeof value === "string")
      return value.replace(/\{\{(\w+)\}\}/g, (_, key) => {
        if (!allowed.includes(key) || typeof auth[key] !== "string")
          throw new Error("Missing or forbidden fixture binding")
        return auth[key]
      })
    if (Array.isArray(value)) return value.map(bind)
    if (value && typeof value === "object")
      return Object.fromEntries(
        Object.entries(value).map(([key, value]) => [key, bind(value)])
      )
    return value
  }
  const request = bind(fixture.request)
  const path = operation.path.replace(/\{(\w+)\}/g, (_, key) => {
    const value = key === "workspace" ? auth.workspaceId : request.path?.[key]
    if (typeof value !== "string" || !/^[a-zA-Z0-9_-]+$/.test(value))
      throw new Error("Missing or unsafe fixture identifier")
    return encodeURIComponent(value)
  })
  const query = new URLSearchParams(request.query ?? {}).toString()
  return {
    path: path + (query ? `?${query}` : ""),
    assertions: bind(fixture.assertions),
  }
}

async function main() {
  const option = (name) => {
    const index = process.argv.indexOf(`--${name}`)
    return index < 0 ? undefined : process.argv[index + 1]
  }
  const base = safeLocalURL(option("base-url"))
  const authFile = option("auth-file")
  const output = option("output")
  const database = option("database")
  if (
    !authFile ||
    !output ||
    !/^genesy_assistant_checks_[a-z0-9]+$/.test(database ?? "")
  )
    throw new Error(
      "Explicit synthetic database, auth file and output required"
    )
  const auth = JSON.parse(await readFile(authFile, "utf8"))
  if (!auth.token || !auth.workspaceId)
    throw new Error("Incomplete local authentication")
  const {
    catalog: operations,
    dataset,
    identity: inputsIdentity,
  } = await loadInputs()
  const identity = { ...inputsIdentity, runId: randomUUID() }
  const records = []
  for (const fixture of dataset.cases.filter((item) => item.request)) {
    const operation = operations.find(
      (operation) => operation.id === fixture.operationId
    )
    const request = prepareRead(operation, fixture, auth)
    for (const authenticated of fixture.scenario === "happy"
      ? [true, false]
      : [true]) {
      const start = performance.now()
      let httpStatus = null
      let passed = false
      try {
        const response = await fetch(
          `${base.href.replace(/\/$/, "")}${request.path}`,
          {
            headers: {
              Accept: "application/json",
              ...(authenticated
                ? { Authorization: `Bearer ${auth.token}` }
                : {}),
            },
            redirect: "error",
            signal: AbortSignal.timeout(15000),
          }
        )
        httpStatus = response.status
        const raw = await response.text()
        if (raw.length <= 180000) {
          const body = JSON.parse(raw)
          passed = authenticated
            ? response.status === fixture.expectedStatus &&
              evaluateAssertions(body, request.assertions)
            : response.status === 401
        }
      } catch {
        /* Record failure without response, URL, token or stack. */
      }
      const gates = authenticated
        ? fixture.scenario === "happy"
          ? ["transport", ...(fixture.resultEvidence ? ["result"] : [])]
          : ["validation"]
        : ["authorization"]
      for (const gate of gates)
        records.push({
          ...identity,
          operationId: operation.id,
          caseId: fixture.id,
          gate,
          kind: "real_http",
          passed,
          httpStatus,
          latencyMs: Math.round(performance.now() - start),
          environment: {
            apiOrigin: base.origin,
            database,
            databaseVerification: "operator_declared",
          },
          assertionCount: authenticated ? request.assertions.length : 1,
        })
    }
  }
  await writeFile(
    output,
    records.map((record) => JSON.stringify(record)).join("\n") + "\n",
    { mode: 0o600, flag: "wx" }
  )
  const failed = records.filter((record) => !record.passed).length
  process.stdout.write(
    JSON.stringify({
      records: records.length,
      passed: records.length - failed,
      failed,
      providerCalls: 0,
      note: "Read-only pilot; does not prove bridge, tenant isolation, restricted/farm roles or complete capability coverage.",
    }) + "\n"
  )
  if (failed) process.exitCode = 1
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
)
  main().catch(() => {
    process.stderr.write(
      "Local read pilot failed. Check arguments, local stack and synthetic fixture bindings; no credentials or response data printed.\n"
    )
    process.exitCode = 1
  })
