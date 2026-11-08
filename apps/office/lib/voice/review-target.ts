import "server-only"
import { randomUUID } from "node:crypto"
import {
  getOperation,
  isToolAllowed,
  operationManifest,
  prepareAction,
  type PreparedAction,
} from "./catalog"
import { callBackend, VoiceError, type VoiceContext } from "./server"
import en from "../../messages/en.json"
import nl from "../../messages/nl.json"
import sw from "../../messages/sw.json"

const resources: Record<string, Record<string, string>> = {
  en: en.voice.resources,
  nl: nl.voice.resources,
  sw: sw.voice.resources,
}
/** A record's own name, first present in this order; blank counts as absent. */
const labelFields = [
  "name",
  "title",
  "company_name",
  "alias",
  "full_name",
  "number",
  "relation_nr",
  "reference",
]
function recordLabel(record: Record<string, unknown>): string | null {
  const contacts = (record.relationships as { contacts?: unknown } | undefined)
    ?.contacts
  const contact = Array.isArray(contacts) ? contacts[0] : undefined
  const candidates = labelFields.flatMap((key) =>
    key === "number"
      ? [(contact as Record<string, unknown> | undefined)?.full_name, record[key]]
      : [record[key]]
  )
  for (const value of candidates)
    if (
      (typeof value === "string" && value.trim()) ||
      (typeof value === "number" && Number.isFinite(value))
    )
      return String(value).trim()
  return null
}

/** Exact route identities remain visible even when a permitted display-name lookup fails. */
export async function reviewTarget(
  action: PreparedAction,
  context: VoiceContext
): Promise<Record<string, unknown>> {
  const operation = getOperation(action.operationId)
  if (!operation)
    throw new VoiceError("This review operation is unavailable.", 409)
  const actual = new URL(action.path, "https://genesis.invalid").pathname.split(
    "/"
  )
  const template = operation.path.split("/")
  const target: Record<string, string> = {}
  if (actual.length !== template.length)
    throw new VoiceError("This review target is invalid.", 409)
  template.forEach((part, index) => {
    if (part === "{workspace}") {
      if (actual[index] !== context.workspaceId)
        throw new VoiceError("This review belongs to another workspace.", 403)
    } else if (part.startsWith("{")) {
      target[part.slice(1, -1)] = decodeURIComponent(actual[index]!)
    } else if (part !== actual[index])
      throw new VoiceError("This review target is invalid.", 409)
  })
  // The resource is the path segment before the operation's last identity.
  const segments = operation.path.split("/")
  const last = segments.reduce(
    (found, part, index) =>
      part.startsWith("{") && part !== "{workspace}" ? index : found,
    -1
  )
  const names = resources[context.language ?? "en"] ?? resources.en!
  const type = names[segments[last - 1] ?? ""] ?? names.record!
  const details: Record<string, unknown> = {
    workspace: context.workspaceId,
    target,
    record: { type, label: null },
  }
  const lookup = operationManifest
    .filter(
      (candidate) =>
        candidate.method === "GET" &&
        !candidate.write &&
        candidate.disposition === "execute" &&
        /\/\{(?!workspace\})[^}]+\}$/.test(candidate.path) &&
        (operation.path === candidate.path ||
          operation.path.startsWith(candidate.path + "/")) &&
        !candidate.parameters.some(
          (parameter) => parameter.in === "query" && parameter.required
        ) &&
        isToolAllowed(candidate, context.permissions)
    )
    .sort((a, b) => b.path.length - a.path.length)[0]
  if (!lookup) return details
  try {
    const path = Object.fromEntries(
      lookup.parameters
        .filter((parameter) => parameter.in === "path")
        .map((parameter) => [parameter.name, target[parameter.name]!])
    )
    const result = await callBackend(
      prepareAction(lookup.id, { path }, context.workspaceId),
      context,
      randomUUID()
    )
    if (
      result.status !== "completed" ||
      !result.data ||
      typeof result.data !== "object" ||
      !("data" in result.data)
    )
      return details
    const record = result.data.data
    const identity = lookup.path.split("/").at(-1)!.slice(1, -1)
    if (
      !record ||
      typeof record !== "object" ||
      Array.isArray(record) ||
      !("id" in record) ||
      String(record.id) !== target[identity]
    )
      return details
    return {
      ...details,
      record: { type, label: recordLabel(record as Record<string, unknown>) },
    }
  } catch {
    return details
  }
}
