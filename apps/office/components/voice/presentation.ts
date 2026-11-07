import { taskPresentation } from "./helpers"

/** Activity is independent of the send button and never infers a native write succeeded. */
export function activityState(state: {
  connected?: boolean
  starting?: boolean
  upgrading?: boolean
  thinking?: boolean
  requesting?: boolean
  uploading?: boolean
  busy?: boolean
  review?: boolean
  error?: string
  outcome?: string
  speaking?: boolean
  textOnly?: boolean
  capturePaused?: boolean
  muted?: boolean
  checkingRecovery?: boolean
}) {
  if (state.error)
    return state.outcome &&
      [
        "queued",
        "partial",
        "unknown",
        "completed",
        "cancelled",
        "expired",
        "paused",
      ].includes(state.outcome)
      ? taskPresentation(state.outcome)
      : "failed"
  if (state.upgrading) return "upgrading"
  if (state.starting) return "connecting"
  if (state.uploading) return "uploading"
  if (state.review && !state.busy) return "awaiting_review"
  if (state.busy || state.requesting || state.outcome === "running")
    return "executing"
  if (state.thinking) return "thinking"
  if (state.outcome) return taskPresentation(state.outcome)
  if (state.checkingRecovery) return "checking"
  if (!state.connected) return "idle"
  if (state.capturePaused) return "capturePaused"
  if (state.muted) return "muted"
  if (state.speaking) return "speaking"
  return state.textOnly ? "ready" : "listening"
}

const uuid =
  /\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b/gi
export type DisplayReferences = Map<string, { number: number; label?: string }>

/** Use only names already projected by authorized reads; signed review data stays untouched. */
export function displayReferences(value: unknown): DisplayReferences {
  const references: DisplayReferences = new Map()
  function visit(value: unknown, depth = 0) {
    if (depth > 32 || value == null) return
    if (typeof value === "string") {
      for (const [id] of value.matchAll(uuid)) {
        const key = id.toLowerCase()
        if (!references.has(key))
          references.set(key, { number: references.size + 1 })
      }
    } else if (Array.isArray(value))
      value.forEach((item) => visit(item, depth + 1))
    else if (typeof value === "object") {
      const record = value as Record<string, unknown>
      Object.values(record).forEach((item) => visit(item, depth + 1))
      if (typeof record.id === "string") {
        const ref = references.get(record.id.toLowerCase())
        const labels = [
          ...new Set(
            [record.name, record.title, record.reference, record.number].filter(
              (item) =>
                (typeof item === "string" && item.trim()) ||
                typeof item === "number"
            )
          ),
        ]
        if (ref && labels.length) ref.label = labels.join(" · ")
      }
    }
  }
  visit(value)
  return references
}

export function displayText(
  value: string,
  references: DisplayReferences,
  fallback: (number: number) => string
) {
  return value.replace(uuid, (id) => {
    const ref = references.get(id.toLowerCase())
    return ref?.label
      ? ref.label.replace(uuid, () => fallback(ref.number))
      : fallback(ref?.number ?? references.size + 1)
  })
}

const resources = [
  "app_keys",
  "email_profiles",
  "supply_links",
  "product_variants",
  "farmer_groups",
  "workspaces",
  "warehouses",
  "products",
  "categories",
  "invoices",
  "orders",
  "contacts",
  "customers",
  "suppliers",
  "farmers",
  "farms",
  "teams",
  "users",
  "members",
  "invites",
  "files",
  "folders",
  "lots",
  "stock",
  "till",
  "messages",
  "webhooks",
  "payments",
  "currencies",
  "vat",
  "workspace",
  "settings",
] as const
const verbs = [
  "send",
  "cancel",
  "refund",
  "approve",
  "reject",
  "publish",
  "release",
  "recall",
  "duplicate",
  "create",
  "update",
  "delete",
  "remove",
  "restore",
  "sync",
  "book",
  "adjust",
  "assign",
  "link",
  "unlink",
  "close",
  "open",
  "pay",
] as const
export function operationLabel(operationId: string, method?: string) {
  const resource =
    resources.find((resource) => operationId.startsWith(resource + "_")) ??
    "record"
  const verb = verbs.find((verb) => operationId.endsWith("_" + verb))
  const action =
    method === "DELETE"
      ? "delete"
      : verb === "remove"
        ? "delete"
        : (verb ?? (method === "PUT" || method === "PATCH" ? "update" : "run"))
  return { action, resource }
}
