import "server-only"
import Ajv2020, { type ValidateFunction } from "ajv/dist/2020"
import addFormats from "ajv-formats"
import generated from "./catalog.generated.json"
import schemas from "./catalog.schemas.generated.json"
import keywords from "./keywords.json"

type Schema = Record<string, unknown>
export type Authorization = {
  schemes: string[]
  requirements: Record<string, string[]>[]
  boundary: string
  allOf: string[]
  anyOf: string[]
  predicates: string[]
  roleAnyOf: string[]
  authority: string[]
}
export type Tool = {
  id: string
  domain: string
  summary: string
  description: string
  method: string
  path: string
  disposition: "execute" | "secure_ui" | "external" | "identity" | "internal"
  availability?: "unavailable"
  unavailableReason?: string
  reason: string
  authorization: Authorization
  effects: string[]
  risk: "read" | "write" | "high" | "secret"
  handoff: {
    route: string | null
    operationId: string
    kind?: "credentials" | "physical_verification"
  } | null
  reconciliation: string[]
  retry: {
    mode: "idempotency_key" | "client_reference" | "reconcile_only"
    field?: string
  }
  confirmationFields: string[]
  attachmentFields?: string[]
  write: boolean
  idempotent: boolean
  /** Compatibility only; authorization.allOf owns conjunctions. */
  permission: string | null
  parameters: {
    name: string
    in: string
    required?: boolean
    description?: string
    schema: Schema
  }[]
  body: Schema | null
  requestRequired: boolean
  requestMedia: Record<string, { schema: Schema }>
  successResponses: Record<
    string,
    { description: string; content?: Record<string, { schema?: Schema }> }
  >
}
export type ActionArguments = {
  path?: Record<string, string>
  query?: Record<string, string | number | boolean>
  body?: unknown
  /** Session-owned app handles, resolved by server; never URLs, base64 or provider ids. */
  attachments?: Record<string, string[]>
}
export type PreparedAction = {
  operationId: string
  summary: string
  method: string
  path: string
  body?: unknown
  attachments?: Record<string, string[]>
  contentType?: string
  write: boolean
  idempotent: boolean
  authorization?: Authorization
  risk?: Tool["risk"]
  effects?: string[]
  reconciliation?: string[]
  successStatuses?: number[]
  retry?: Tool["retry"]
  confirmationFields?: string[]
}

export const operationManifest = generated as unknown as Tool[]
export const tools = operationManifest.filter(
  (op) => op.disposition === "execute"
)
const byId = new Map(operationManifest.map((op) => [op.id, op]))
export const getOperation = (id: string): Tool | undefined => byId.get(id)
// Trigger words (en/nl/sw) are matched by discovery only; never shown on screen.
const triggerWords = new Map(
  Object.entries(keywords as Record<string, Record<string, string[]>>).map(
    ([id, words]) => [id, Object.values(words).flat().join(" ")]
  )
)

/** Safe refusal for inventoried capabilities missing native backend behavior. */
export class ToolUnavailableError extends Error {
  readonly status = 501
  readonly code = "unavailable"
  constructor() {
    super("This capability is unavailable in the current backend.")
  }
}

/** Claims come only from authenticated /me and same-workspace /farm/me; Rust rechecks resource scope. */
export function isToolAllowed(tool: Tool, permissions: string[]): boolean {
  const admin = permissions.some((p) =>
    ["workspace_owner", "workspace_admin"].includes(p)
  )
  const allows = (grant: string) =>
    permissions.includes(grant) || (admin && !grant.startsWith("farm:"))
  return (
    tool.authorization.allOf.every(allows) &&
    (!tool.authorization.anyOf.length ||
      tool.authorization.anyOf.some(allows)) &&
    (!tool.authorization.roleAnyOf.length ||
      tool.authorization.roleAnyOf.some((role) =>
        permissions.includes(`farm:${role}`)
      ))
  )
}

/** Discover secure capabilities without ever putting credential schemas into model context. */
function discoveryContract(tool: Tool, exact = false): Tool {
  const secure = tool.disposition === "secure_ui"
  const unavailable = tool.availability === "unavailable"
  if (!exact || secure || unavailable)
    return {
      ...tool,
      description: unavailable
        ? "This operation is unavailable in the current backend. Do not prepare or execute it."
        : secure
          ? tool.reason
          : "Discover this exact operation ID to load its input contract before preparing arguments.",
      body: null,
      requestMedia: {},
      successResponses: {},
      parameters: secure ? tool.parameters.filter((p) => p.in === "path") : [],
    }
  const multipart = tool.requestMedia["multipart/form-data"]?.schema
  const properties = multipart?.properties as Record<string, Schema> | undefined
  return {
    ...tool,
    body: multipart ?? tool.body,
    attachmentFields: Object.entries(properties ?? {})
      .filter(([, schema]) => schema.format === "binary")
      .map(([field]) => field),
    requestMedia: Object.fromEntries(
      Object.keys(tool.requestMedia).map((media) => [media, { schema: {} }])
    ),
    successResponses: Object.fromEntries(
      Object.entries(tool.successResponses).map(([status, response]) => [
        status,
        {
          description: response.description,
          ...(response.content
            ? {
                content: Object.fromEntries(
                  Object.keys(response.content).map((media) => [media, {}])
                ),
              }
            : {}),
        },
      ])
    ),
  }
}

/** Search contracts on demand; secure flows resolve to an explicit native UI handoff. */
export function discoverTools(
  query: string,
  permissions: string[] = []
): Tool[] {
  const allowed = operationManifest.filter(
    (tool) =>
      ["execute", "secure_ui"].includes(tool.disposition) &&
      isToolAllowed(tool, permissions)
  )
  const exact = byId.get(query)
  if (exact)
    return allowed.includes(exact) ? [discoveryContract(exact, true)] : []
  const terms = query
    .toLowerCase()
    .replace(/[_-]/g, " ")
    .split(/\s+/)
    .filter(Boolean)
  return (
    allowed
      .map((tool) => ({
        tool,
        score: terms.reduce(
          (score, term) =>
            score +
            (`${tool.id} ${tool.domain} ${tool.summary} ${triggerWords.get(tool.id) ?? ""}`
              .replace(/[_-]/g, " ")
              .toLowerCase()
              .includes(term)
              ? 1
              : 0),
          0
        ),
      }))
      .filter((item) => item.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 8)
      // Top match carries its full contract so execute can follow a single search.
      .map(({ tool }, index) => discoveryContract(tool, index === 0))
  )
}

const ajv = new Ajv2020({
  strict: false,
  allErrors: false,
  validateFormats: true,
  allowUnionTypes: true,
})
addFormats(ajv)
for (const format of [
  "password",
  "binary",
  "int32",
  "int64",
  "double",
  "float",
])
  ajv.addFormat(format, true)
const schemaId = "urn:genesy:voice-contracts"
ajv.addSchema({ $id: schemaId, components: schemas.components })
function references(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(references)
  if (!object(value)) return value
  return Object.fromEntries(
    Object.entries(value).map(([key, child]) => [
      key,
      key === "$ref" &&
      typeof child === "string" &&
      child.startsWith("#/components/")
        ? `${schemaId}${child}`
        : references(child),
    ])
  )
}
const validators = new Map<string, ValidateFunction>()
function validator(key: string, schema: Schema) {
  let validate = validators.get(key)
  if (!validate) {
    validate = ajv.compile(references(schema) as Schema)
    validators.set(key, validate)
  }
  return validate
}
function validateValue(key: string, schema: Schema, value: unknown) {
  const validate = validator(key, schema)
  if (!validate(value)) {
    const error = validate.errors?.[0]
    throw new Error(
      `Invalid ${key}${error?.instancePath ?? ""}: ${error?.message ?? "schema mismatch"}`
    )
  }
}

function object(value: unknown): value is Record<string, unknown> {
  return (
    value !== null &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    [Object.prototype, null].includes(Object.getPrototypeOf(value))
  )
}
function safeNumbers(value: unknown, depth = 0) {
  if (depth > 32) throw new Error("Arguments are nested too deeply.")
  if (
    typeof value === "number" &&
    (!Number.isFinite(value) || Math.abs(value) > Number.MAX_SAFE_INTEGER)
  )
    throw new Error(
      "Numbers must be finite and within exact JavaScript range; use the API's decimal text fields for money."
    )
  if (Array.isArray(value))
    for (const child of value) safeNumbers(child, depth + 1)
  else if (object(value))
    for (const child of Object.values(value)) safeNumbers(child, depth + 1)
}

/** Reject invented fields where API gives a closed set, retaining documented free-form metadata. */
function knownFields(schema: Schema, value: unknown, at = "body") {
  if (Array.isArray(value) && object(schema.items))
    for (const child of value) knownFields(schema.items, child, `${at}[]`)
  if (!object(value)) return
  if (object(schema.properties) && !schema.additionalProperties) {
    for (const [key, child] of Object.entries(value)) {
      if (!Object.hasOwn(schema.properties, key))
        throw new Error(`Unknown ${at} field: ${key}`)
      const childSchema = schema.properties[key]
      if (object(childSchema)) knownFields(childSchema, child, `${at}.${key}`)
    }
  }
}

function typedQuery(value: string | number | boolean, schema: Schema) {
  const types = Array.isArray(schema.type) ? schema.type : [schema.type]
  if (
    typeof value === "string" &&
    (types.includes("integer") || types.includes("number"))
  ) {
    if (!/^-?(?:0|[1-9]\d*)(?:\.\d+)?$/.test(value)) return value
    return Number(value)
  }
  if (types.includes("boolean") && ["true", "false"].includes(String(value)))
    return String(value) === "true"
  return value
}

function prepare(
  operationId: string,
  value: unknown,
  workspace: string,
  secure: boolean
): PreparedAction {
  const tool = byId.get(operationId)
  if (!tool || tool.disposition !== (secure ? "secure_ui" : "execute"))
    throw new Error("This operation is not available to the voice assistant.")
  if (tool.availability === "unavailable") throw new ToolUnavailableError()
  if (!object(value)) throw new Error("Arguments must be a JSON object.")
  if (
    Object.keys(value).some(
      (key) => !["path", "query", "body", "attachments"].includes(key)
    )
  )
    throw new Error("Only path, query, body and attachments are accepted.")
  safeNumbers(value)
  const args = value as ActionArguments
  const path = args.path ?? {}
  const query = args.query ?? {}
  if (!object(path) || !object(query))
    throw new Error("Path and query must be objects.")
  validateValue("workspace", { type: "string", format: "uuid" }, workspace)
  for (const [group, input] of [
    ["path", path],
    ["query", query],
  ] as const) {
    for (const name of Object.keys(input))
      if (!tool.parameters.some((p) => p.in === group && p.name === name))
        throw new Error(`Unknown ${group} parameter: ${name}`)
  }
  let urlPath = tool.path.replace("{workspace}", encodeURIComponent(workspace))
  for (const param of tool.parameters.filter((p) => p.in === "path")) {
    const v = path[param.name]
    if (
      typeof v !== "string" ||
      !v ||
      /[\s/\\?#%]/.test(v) ||
      [".", ".."].includes(v)
    )
      throw new Error(`A valid ${param.name} is required.`)
    validateValue(`${operationId}.path.${param.name}`, param.schema, v)
    urlPath = urlPath.replace(`{${param.name}}`, encodeURIComponent(v))
  }
  const search = new URLSearchParams()
  for (const [name, value] of Object.entries(query)) {
    if (!["string", "number", "boolean"].includes(typeof value))
      throw new Error(`Invalid filter: ${name}`)
    const param = tool.parameters.find(
      (p) => p.in === "query" && p.name === name
    )!
    const typed = typedQuery(value, param.schema)
    safeNumbers(typed)
    validateValue(`${operationId}.query.${name}`, param.schema, typed)
    search.set(name, String(value))
  }
  for (const [name, cap] of [
    ["page_size", 20],
    ["limit", 10],
  ] as const)
    if (tool.parameters.some((p) => p.in === "query" && p.name === name))
      search.set(
        name,
        String(Math.max(1, Math.min(cap, Number(query[name]) || cap)))
      )
  for (const param of tool.parameters.filter(
    (p) => p.in === "query" && p.required
  ))
    if (!search.has(param.name))
      throw new Error(`Missing filter: ${param.name}`)
  if (search.size) urlPath += `?${search}`

  const multipart = tool.requestMedia["multipart/form-data"]
  const contentType = multipart
    ? "multipart/form-data"
    : tool.requestMedia["application/json"]
      ? "application/json"
      : undefined
  let body = args.body
  if (!contentType && body !== undefined)
    throw new Error("This operation does not accept a body.")
  if (
    args.attachments !== undefined &&
    (!multipart || !object(args.attachments))
  )
    throw new Error("This operation does not accept these attachments.")
  if (multipart) {
    if (body !== undefined && !object(body))
      throw new Error("Multipart fields must be an object.")
    const fields: Record<string, unknown> = {
      ...(body as Record<string, unknown> | undefined),
    }
    const properties = multipart.schema.properties as
      Record<string, Schema> | undefined
    for (const [field, handles] of Object.entries(args.attachments ?? {})) {
      if (
        properties?.[field]?.format !== "binary" ||
        !Array.isArray(handles) ||
        handles.length !== 1 ||
        !handles.every(
          (handle) =>
            typeof handle === "string" && /^[a-zA-Z0-9_-]{1,160}$/.test(handle)
        )
      )
        throw new Error(`A session attachment handle is required for ${field}.`)
      if (Object.hasOwn(fields, field))
        throw new Error(`File bytes must not appear in body.${field}.`)
      fields[field] = handles[0]
    }
    for (const [field, schema] of Object.entries(properties ?? {}))
      if (
        schema.format === "binary" &&
        Object.hasOwn(fields, field) &&
        !args.attachments?.[field]
      )
        throw new Error(`Use a session attachment handle for ${field}.`)
    if (
      operationId === "farm_bug_reports_create" &&
      !fields.photo &&
      !fields.screenshot
    )
      throw new Error("A screenshot or photo attachment is required.")
    validateValue(`${operationId}.multipart`, multipart.schema, fields)
    knownFields(multipart.schema, fields)
    body = args.body ?? {}
  } else if (contentType) {
    if (tool.requestRequired && body === undefined)
      throw new Error("A request body is required.")
    if (body !== undefined) {
      const schema = tool.requestMedia[contentType]!.schema
      validateValue(`${operationId}.body`, schema, body)
      knownFields(schema, body)
    }
  }
  return {
    operationId,
    summary: tool.summary,
    method: tool.method,
    path: urlPath,
    ...(body !== undefined ? { body } : {}),
    ...(args.attachments ? { attachments: args.attachments } : {}),
    ...(contentType ? { contentType } : {}),
    write: tool.write,
    idempotent: tool.idempotent,
    authorization: tool.authorization,
    risk: tool.risk,
    effects: tool.effects,
    reconciliation: tool.reconciliation,
    successStatuses: Object.keys(tool.successResponses).map(Number),
    retry: tool.retry,
    confirmationFields: tool.confirmationFields,
  }
}

/** Prepare model-visible business calls; secure operations require a separate native endpoint. */
export function prepareAction(
  operationId: string,
  value: unknown,
  workspace: string
): PreparedAction {
  return prepare(operationId, value, workspace, false)
}

/** Native paused-conversation form only; never expose through assistant tool execution. */
export function prepareSecureAction(
  operationId: string,
  value: unknown,
  workspace: string
): PreparedAction {
  const action = prepare(operationId, value, workspace, true)
  if (
    getOperation(operationId)?.handoff?.kind === "physical_verification" &&
    (!object(action.body) || action.body.name_confirmed !== true)
  )
    throw new Error(
      "Confirm the physical ID and name check in this secure form."
    )
  return action
}

/** Check successful JSON/empty responses against authoritative contract, before summarizing. */
export function validateToolResponse(
  operationId: string,
  status: number,
  contentType: string,
  value: unknown
) {
  const tool = byId.get(operationId)
  const response = tool?.successResponses[String(status)]
  if (!response)
    throw new Error("The backend returned an undocumented success status.")
  if (!response.content || !Object.keys(response.content).length) {
    if (value !== null && value !== undefined && value !== "")
      throw new Error("The backend returned a body for an empty response.")
    return
  }
  const schema = response.content[contentType.split(";")[0]!.trim()]?.schema
  if (!schema)
    throw new Error("The backend returned an undocumented content type.")
  safeNumbers(value)
  validateValue(`${operationId}.response.${status}`, schema, value)
}
