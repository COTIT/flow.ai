// Contracts come from OpenAPI; reviewed policy owns exposure, authorization and effects.
import { readFile, writeFile } from "node:fs/promises"
import { createHash } from "node:crypto"
import { fileURLToPath } from "node:url"

function expand(value, spec, seen = new Set()) {
  if (!value || typeof value !== "object") return value
  if (value.$ref) {
    if (seen.has(value.$ref))
      throw new Error(`Recursive tool schema: ${value.$ref}`)
    const target = value.$ref
      .split("/")
      .slice(1)
      .reduce(
        (v, key) => v?.[key.replace(/~1/g, "/").replace(/~0/g, "~")],
        spec
      )
    if (!target) throw new Error(`Unknown schema: ${value.$ref}`)
    const { $ref, ...siblings } = value
    return {
      ...expand(target, spec, new Set([...seen, $ref])),
      ...expand(siblings, spec, seen),
    }
  }
  if (Array.isArray(value)) return value.map((v) => expand(v, spec, seen))
  return Object.fromEntries(
    Object.entries(value).map(([k, v]) => [k, expand(v, spec, seen)])
  )
}

/** Every served contract needs an explicit reviewed disposition; unknown business routes fail closed. */
export function buildCatalog(spec, policies) {
  // Only source-reviewed native no-ops may be removed from advertised inputs.
  const ignoredProperties = {
    invoices_create: ["/properties/due_date", "/properties/customer_ref"],
    invoices_update: [
      "/properties/currency_id",
      "/properties/payment_method",
      "/properties/is_credit",
    ],
    email_templates_update: ["/properties/template_domain"],
    invoices_credit: ["/properties/items/items/properties/disable_discount"],
  }
  const ids = new Set()
  const operations = []
  const readPosts = new Set([
    "pricing_calculate_items",
    "pricing_calculate_marge",
    "farm_pue_requests_estimate",
    "farm_vouchers_lookup",
    "till_sales_quote",
    "till_shift_count_quote",
  ])
  const security = (requirements) =>
    (requirements ?? [])
      .map((entry) =>
        JSON.stringify(
          Object.fromEntries(
            Object.entries(entry).sort(([a], [b]) => a.localeCompare(b))
          )
        )
      )
      .sort()
      .join("|")
  for (const [path, methods] of Object.entries(spec.paths)) {
    for (const [method, op] of Object.entries(methods)) {
      if (!op.operationId) continue
      const id = op.operationId
      if (ids.has(id)) throw new Error(`Duplicate operation: ${id}`)
      ids.add(id)
      const policy = policies[id]
      if (!policy) throw new Error(`Unreviewed voice operation: ${id}`)
      if (
        !["execute", "secure_ui", "external", "identity", "internal"].includes(
          policy.disposition
        )
      )
        throw new Error(`Invalid voice disposition: ${id}`)
      if (!["read", "write", "high", "secret"].includes(policy.risk))
        throw new Error(`Invalid voice risk: ${id}`)
      if (policy.path !== path || policy.method !== method.toUpperCase())
        throw new Error(`Voice policy route changed: ${id}`)
      const schemes = (op.security ?? []).flatMap((security) =>
        Object.keys(security)
      )
      if (
        JSON.stringify(schemes) !== JSON.stringify(policy.authorization.schemes)
      )
        throw new Error(`Voice policy identity changed: ${id}`)
      if (security(op.security) !== security(policy.authorization.requirements))
        throw new Error(`Voice policy security requirements changed: ${id}`)
      if (!policy.authorization.authority.length)
        throw new Error(`Missing authorization authority: ${id}`)
      if (
        ["execute", "secure_ui"].includes(policy.disposition) &&
        (JSON.stringify(schemes) !== '["bearer"]' ||
          !path.startsWith("/workspace/{workspace}"))
      )
        throw new Error(`Non-staff operation exposed: ${id}`)
      if (policy.disposition !== "internal" && /\/assistant(?:\/|$)/.test(path))
        throw new Error(`Assistant control exposed as business tool: ${id}`)
      if (
        policy.disposition === "execute" &&
        policy.effects.includes("credential")
      )
        throw new Error(`Credential operation exposed: ${id}`)
      if (
        policy.disposition === "execute" &&
        method !== "get" &&
        !policy.effects.length &&
        !readPosts.has(id)
      )
        throw new Error(`Unreviewed read-only mutation method: ${id}`)
      if (
        policy.disposition === "secure_ui" &&
        (!policy.handoff ||
          !(
            policy.effects.includes("credential") ||
            (policy.handoff.kind === "physical_verification" &&
              policy.confirmationFields.includes("name_confirmed"))
          ))
      )
        throw new Error(`Missing secure handoff: ${id}`)
      if (
        policy.availability !== undefined &&
        policy.availability !== "unavailable"
      )
        throw new Error(`Invalid reviewed availability: ${id}`)
      if (
        (policy.availability === "unavailable" &&
          (typeof policy.unavailableReason !== "string" ||
            !/^[a-z][a-z0-9_]{0,100}$/.test(policy.unavailableReason))) ||
        (policy.unavailableReason !== undefined &&
          policy.availability !== "unavailable")
      )
        throw new Error(`Missing reviewed unavailable reason: ${id}`)
      const parameters = expand(
        (op.parameters ?? []).filter(
          (p) => p.name !== "workspace" && p.in !== "header"
        ),
        spec
      )
      const requestMedia = expand(op.requestBody?.content ?? {}, spec)
      const {
        ignoredBodyFields = [],
        ignoredBodySchemaProperties = [],
        ...reviewedPolicy
      } = policy
      if (
        !Array.isArray(ignoredBodyFields) ||
        ignoredBodyFields.some((field) => typeof field !== "string") ||
        new Set(ignoredBodyFields).size !== ignoredBodyFields.length ||
        !Array.isArray(ignoredBodySchemaProperties) ||
        ignoredBodySchemaProperties.some(
          (pointer) => typeof pointer !== "string"
        ) ||
        new Set(ignoredBodySchemaProperties).size !==
          ignoredBodySchemaProperties.length
      )
        throw new Error(`Invalid ignored body fields: ${id}`)
      const omitted = [
        ...ignoredBodyFields.map(
          (field) =>
            `/properties/${field.replaceAll("~", "~0").replaceAll("/", "~1")}`
        ),
        ...ignoredBodySchemaProperties,
      ]
      for (const pointer of omitted) {
        if (!ignoredProperties[id]?.includes(pointer))
          throw new Error(`Unreviewed ignored body property: ${id}${pointer}`)

        const parts = pointer
          .split("/")
          .slice(1)
          .map((part) => part.replace(/~1/g, "/").replace(/~0/g, "~"))
        if (
          !pointer.startsWith("/properties/") ||
          parts.at(-2) !== "properties"
        )
          throw new Error(`Invalid ignored schema property: ${id}${pointer}`)
        const schema = parts
          .slice(0, -2)
          .reduce(
            (value, key) =>
              value && typeof value === "object" && Object.hasOwn(value, key)
                ? value[key]
                : undefined,
            requestMedia["application/json"]?.schema
          )
        const field = parts.at(-1)
        if (
          !schema?.properties ||
          !Object.hasOwn(schema.properties, field) ||
          schema.required?.includes(field)
        )
          throw new Error(
            `Reviewed ignored body field changed: ${id}${pointer}`
          )
        delete schema.properties[field]
      }
      operations.push({
        id,
        domain: op.tags?.[0] ?? "other",
        summary: op.summary ?? id,
        description: op.description ?? "",
        method: method.toUpperCase(),
        path,
        ...reviewedPolicy,
        write: policy.effects.length > 0,
        idempotent: (op.parameters ?? []).some(
          (p) => p.name.toLowerCase() === "idempotency-key"
        ),
        permission: policy.authorization.allOf[0] ?? null,
        parameters,
        requestRequired: op.requestBody?.required ?? false,
        requestMedia,
        body:
          requestMedia["application/json"]?.schema ??
          requestMedia["multipart/form-data"]?.schema ??
          null,
        successResponses: Object.fromEntries(
          Object.entries(op.responses).filter(([status]) =>
            /^2\d\d$/.test(status)
          )
        ),
      })
    }
  }
  for (const id of Object.keys(policies))
    if (!ids.has(id))
      throw new Error(`Removed operation still has voice policy: ${id}`)
  for (const op of operations)
    for (const id of op.reconciliation)
      if (!ids.has(id))
        throw new Error(`Unknown reconciliation ${id} on ${op.id}`)
  return operations
}

async function main() {
  const spec = JSON.parse(
    await readFile(new URL("../../api/openapi.json", import.meta.url), "utf8")
  )
  const policies = JSON.parse(
    await readFile(
      new URL("../lib/voice/operation-policy.json", import.meta.url),
      "utf8"
    )
  )
  const authorities = JSON.parse(
    await readFile(
      new URL(
        "../lib/voice/operation-policy.authorities.json",
        import.meta.url
      ),
      "utf8"
    )
  )
  const paths = new Set(
    Object.values(policies).flatMap((policy) =>
      policy.authorization.authority.map((ref) => ref.split("#")[0])
    )
  )
  for (const path of [
    "apps/api/src/http/auth.rs",
    "apps/api/src/http/workspace.rs",
    "apps/api/src/http/farm.rs",
    "apps/api/src/modules/permissions/model.rs",
    "apps/api/src/modules/permissions/catalog.rs",
    "apps/api/src/idempotency.rs",
  ])
    paths.add(path)
  for (const path of Object.keys(authorities))
    if (!paths.has(path)) throw new Error(`Unused policy authority: ${path}`)
  await Promise.all(
    [...paths].map(async (path) => {
      if (!path.startsWith("apps/api/src/") || path.includes(".."))
        throw new Error(`Invalid policy authority: ${path}`)
      const bytes = await readFile(new URL(`../../../${path}`, import.meta.url))
      if (
        createHash("sha256").update(bytes).digest("hex") !== authorities[path]
      )
        throw new Error(
          `Authorization source changed; review voice policy: ${path}`
        )
    })
  )
  const operations = buildCatalog(spec, policies)
  const outputs = [
    ["catalog.generated.json", operations],
    ["catalog.schemas.generated.json", { components: spec.components }],
  ]
  for (const [name, value] of outputs) {
    const target = new URL(`../lib/voice/${name}`, import.meta.url)
    const output = JSON.stringify(value) + "\n"
    if (process.argv.includes("--check")) {
      if ((await readFile(target, "utf8")) !== output)
        throw new Error(
          "Voice tool contracts are stale. Run pnpm --filter @genesy/office voice:generate."
        )
    } else await writeFile(target, output)
  }
  console.log(
    `Classified ${operations.length} operations; ${operations.filter((op) => op.disposition === "execute").length} staff tools; ${operations.filter((op) => op.availability === "unavailable").length} unavailable.`
  )
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1])
  await main()
