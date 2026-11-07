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
  const details: Record<string, unknown> = {
    workspace: context.workspaceId,
    target,
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
    const projected = Object.fromEntries(
      Object.entries(record).filter(
        ([key, value]) =>
          ["id", "name", "title", "reference", "number"].includes(key) &&
          (typeof value === "string" ||
            (typeof value === "number" && Number.isFinite(value)))
      )
    )
    return { ...details, record: projected }
  } catch {
    return details
  }
}
