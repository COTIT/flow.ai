import "server-only"

import {
  getOperation,
  ToolUnavailableError,
  validateToolResponse,
  type PreparedAction,
} from "./catalog"
import { readBounded } from "./bridge"
import { recordLinks, type RecordLink } from "./links"
import { resultView, type ResultView } from "@genesy/voice/lib/views"
import { failureReason } from "@genesy/voice/lib/client-contract"
import { noteVoiceFailure } from "./tracing"
import { voiceSetting } from "./settings"

export type VoiceContext = {
  token: string
  workspaceId: string
  permissions: string[]
  apiUrl: string
  sessionId?: string
  language?: string
  nativeSecure?: boolean
  batch?: { id: string; token: string }
  transport?: { bytes: Uint8Array; contentType: string }
}
export type Outcome = {
  status:
    "completed" | "failed" | "unknown" | "cancelled" | "partial" | "queued"
  data?: unknown
  message?: string
  actionId?: string
  /** Office pages of the records a completed write created or changed. */
  links?: RecordLink[]
  /** A chart or cards for a read result; shown, never spoken. */
  view?: ResultView
  /** Why it did not complete: a `failureReason` key, for the agent and monitoring. */
  reason?: string
  httpStatus?: number
  /** Request fields the backend refused. */
  fields?: string[]
}

const mailOutcomes: Record<string, Pick<Outcome, "status" | "message">> = {
  accepted: {
    status: "queued",
    message:
      "Email accepted by the mail server. Delivery is not yet confirmed.",
  },
  blocked: { status: "failed", message: "Email was blocked and was not sent." },
  manual: {
    status: "partial",
    message:
      "A manual email step is required. Email has not been sent automatically.",
  },
  no_recipients: {
    status: "failed",
    message: "Email was not sent because no recipients were selected.",
  },
}
const invoiceOutcomes: Record<string, Pick<Outcome, "status" | "message">> = {
  queued: {
    status: "queued",
    message: "Invoice is numbered. Delivery is queued.",
  },
  accepted: {
    status: "queued",
    message:
      "Invoice is numbered. Email was accepted by the mail server; delivery is not yet confirmed.",
  },
  blocked: {
    status: "partial",
    message:
      "Invoice is numbered, but its email was blocked. Check the delivery record before repeating.",
  },
  manual: {
    status: "partial",
    message: "Invoice is numbered. A manual email step is required.",
  },
  failed: {
    status: "partial",
    message:
      "Delivery failed after invoice numbering. Check the delivery record before repeating.",
  },
  unknown: {
    status: "unknown",
    message:
      "Invoice is numbered; its delivery outcome is uncertain. Check the delivery record before repeating.",
  },
}

export class VoiceError extends Error {
  constructor(
    message: string,
    public readonly status = 400
  ) {
    super(message)
  }
}

/** Validate request shape only; the backend still verifies the bearer. */
export async function voiceBearer(
  request: Request,
  workspaceId: unknown
): Promise<VoiceContext> {
  const auth = request.headers.get("authorization") ?? ""
  if (!/^Bearer \S+$/.test(auth))
    throw new VoiceError("Sign in to use the voice assistant.", 401)
  if (
    typeof workspaceId !== "string" ||
    !/^[a-zA-Z0-9-]{1,100}$/.test(workspaceId)
  )
    throw new VoiceError("Choose a workspace.")
  const apiUrl = (await voiceSetting("API_URL")) || "http://localhost:8080/v1"
  return { token: auth.slice(7), workspaceId, permissions: [], apiUrl }
}

/** Verify the current staff login and workspace on the authoritative backend. */
export async function authorizeVoice(
  request: Request,
  workspaceId: unknown
): Promise<VoiceContext> {
  return verifyLogin(await voiceBearer(request, workspaceId))
}

export async function verifyLogin(base: VoiceContext): Promise<VoiceContext> {
  const { token, workspaceId, apiUrl } = base
  const options = {
    headers: { Authorization: `Bearer ${token}`, Accept: "application/json" },
    cache: "no-store" as const,
    redirect: "error" as const,
    signal: AbortSignal.timeout(15000),
  }
  const [membership, profile, farm] = await Promise.all([
    fetch(`${apiUrl}/me/workspaces`, options),
    fetch(`${apiUrl}/me`, options),
    fetch(`${apiUrl}/farm/me`, options),
  ])
  if (!membership.ok || !profile.ok)
    throw new VoiceError(
      "Your session could not be verified.",
      membership.status === 401 || profile.status === 401 ? 401 : 403
    )
  const workspaces = (await membership.json()) as { data: { id: string }[] }
  if (!workspaces.data?.some((w) => w.id === workspaceId))
    throw new VoiceError("You do not have access to this workspace.", 403)
  const me = (await profile.json()) as {
    data: {
      user: { permissions: { workspace: string; permissions: string[] }[] }
    }
  }
  const permissions = [
    ...(me.data?.user?.permissions?.find((p) => p.workspace === workspaceId)
      ?.permissions ?? []),
  ]
  if (farm.status === 401) throw new VoiceError("Your session has ended.", 401)
  if (farm.ok) {
    const account = (await farm.json()) as {
      data?: { memberships?: { workspace_id?: string; role?: string }[] }
    }
    permissions.push(
      ...farmGrants(
        (account.data?.memberships ?? [])
          .filter((entry) => entry.workspace_id === workspaceId)
          .map((entry) => entry.role)
      )
    )
  }
  return { ...base, permissions }
}

/** Only known farm roles become claims; Rust rechecks resource scope. */
export function farmGrants(roles: unknown[]): string[] {
  return roles
    .filter(
      (role): role is string =>
        typeof role === "string" &&
        ["farmer", "field_officer", "ops", "admin"].includes(role)
    )
    .map((role) => `farm:${role}`)
}

/** No automatic retries: a timeout can occur after the backend applied a write. */
export async function callBackend(
  action: PreparedAction,
  context: VoiceContext,
  actionId: string
): Promise<Outcome> {
  if (getOperation(action.operationId)?.availability === "unavailable")
    return { status: "failed", message: new ToolUnavailableError().message }
  if (action.write && !context.nativeSecure && !context.sessionId)
    throw new VoiceError("Writes require an approved assistant session.", 403)
  const response = await fetch(`${context.apiUrl}${action.path}`, {
    method: action.method,
    redirect: "error",
    cache: "no-store",
    signal: AbortSignal.timeout(25000),
    headers: {
      Accept: "application/json",
      Authorization: `Bearer ${context.token}`,
      ...(context.transport
        ? { "Content-Type": context.transport.contentType }
        : action.body !== undefined
          ? { "Content-Type": "application/json" }
          : {}),
      ...(action.idempotent ? { "Idempotency-Key": `voice-${actionId}` } : {}),
      ...(action.write && !context.nativeSecure
        ? {
            "Assistant-Action-Id": actionId,
            "Assistant-Session-Id": context.sessionId!,
            ...(context.batch
              ? {
                  "Assistant-Batch-Id": context.batch.id,
                  "Assistant-Batch-Token": context.batch.token,
                }
              : {}),
          }
        : {}),
    },
    ...(context.transport
      ? { body: new Blob([context.transport.bytes as Uint8Array<ArrayBuffer>]) }
      : action.body !== undefined
        ? { body: JSON.stringify(action.body) }
        : {}),
  })
  let text: string
  try {
    text = new TextDecoder().decode(await readBounded(response.body, 180000))
  } catch {
    return {
      status: action.write ? "unknown" : "failed",
      message:
        "The result exceeds the safe response size. Check the recorded action before repeating it.",
    }
  }
  let data: unknown
  try {
    data = text ? JSON.parse(text) : null
  } catch {
    return {
      status: action.write ? "unknown" : "failed",
      message:
        "The backend returned an unreadable result. Check Office before repeating a write.",
    }
  }
  if (!response.ok) return refused(action, response.status, data)
  try {
    validateToolResponse(
      action.operationId,
      response.status,
      response.headers.get("content-type") ?? "",
      data
    )
  } catch {
    return {
      status: action.write ? "unknown" : "failed",
      message:
        "The backend response did not match its verified contract. Check the recorded action before repeating it.",
    }
  }
  if (action.operationId === "orders_refund") {
    const refund =
      data && typeof data === "object" && "data" in data ? data.data : undefined
    if (
      refund &&
      typeof refund === "object" &&
      "status" in refund &&
      (refund.status === "creating" || refund.status === "pending")
    )
      return {
        status: "queued",
        // Native amount is minor units without currency metadata; exact money stays on reviewed card.
        data: {
          data: {
            ...("refund_id" in refund ? { refund_id: refund.refund_id } : {}),
            status: refund.status,
          },
        },
        message: "Refund requested. Provider settlement is not yet confirmed.",
      }
    return {
      status: "unknown",
      message:
        "The refund request was processed, but settlement could not be verified. Check the recorded refund before repeating.",
    }
  }
  if (action.operationId === "product_variants_generate") {
    const receipt =
      data && typeof data === "object" && "receipt" in data
        ? data.receipt
        : undefined
    if (
      receipt &&
      typeof receipt === "object" &&
      "status" in receipt &&
      "requested" in receipt &&
      "created" in receipt &&
      "skipped" in receipt &&
      typeof receipt.requested === "number" &&
      typeof receipt.created === "number" &&
      typeof receipt.skipped === "number" &&
      [receipt.requested, receipt.created, receipt.skipped].every(
        (value) => Number.isSafeInteger(value) && value >= 0 && value <= 500
      ) &&
      receipt.created + receipt.skipped === receipt.requested &&
      (receipt.status === "applied" || receipt.status === "partial") &&
      (receipt.status === "partial") === receipt.skipped > 0 &&
      data &&
      typeof data === "object" &&
      "data" in data &&
      Array.isArray(data.data) &&
      data.data.length === receipt.created
    )
      return {
        status: receipt.status === "partial" ? "partial" : "completed",
        data: {
          data: data.data,
          receipt: {
            status: receipt.status,
            requested: receipt.requested,
            created: receipt.created,
            skipped: receipt.skipped,
          },
        },
        message:
          receipt.status === "partial"
            ? "Some requested variants were skipped. Review the created variants before continuing."
            : undefined,
      }
    return {
      status: "unknown",
      message:
        "Variant generation was processed, but its saved count could not be verified. Check variants before repeating.",
    }
  }
  if (
    [
      "messages_send",
      "messages_profile_test",
      "invoices_send",
      "invoices_resend",
    ].includes(action.operationId)
  ) {
    const payload =
      data && typeof data === "object" && "data" in data ? data.data : data
    const receipt =
      payload && typeof payload === "object" && "delivery" in payload
        ? payload.delivery
        : undefined
    const outcomes = action.operationId.startsWith("invoices_")
      ? invoiceOutcomes
      : mailOutcomes
    if (
      receipt &&
      typeof receipt === "object" &&
      "status" in receipt &&
      typeof receipt.status === "string" &&
      Object.hasOwn(outcomes, receipt.status)
    )
      return { ...outcomes[receipt.status]!, data: { delivery: receipt } }
    return {
      status: "unknown",
      data: {
        message:
          "The request was processed, but this response does not distinguish accepted, blocked or manual sending. Check email logs before repeating.",
      },
    }
  }
  if (["teams_create", "teams_set_permissions"].includes(action.operationId)) {
    const payload =
      data && typeof data === "object" && "data" in data ? data.data : data
    const receipt =
      payload && typeof payload === "object" && "permission_changes" in payload
        ? payload.permission_changes
        : undefined
    if (receipt && typeof receipt === "object" && "status" in receipt) {
      if (receipt.status === "partial")
        return {
          status: "partial",
          data: payload,
          message:
            "Some requested team permissions were not applied. Review the receipt before continuing.",
        }
      if (receipt.status === "applied") return completed(action, data)
    }
    return {
      status: "unknown",
      message:
        "The team request was processed, but its permission changes could not be verified. Check the team before repeating.",
    }
  }
  return completed(action, data)
}

/** A refused call, in words the agent can pass on; never the raw status alone. */
function refused(action: PreparedAction, httpStatus: number, data: unknown) {
  const body =
    data && typeof data === "object" ? (data as Record<string, unknown>) : {}
  const fields =
    body.errors && typeof body.errors === "object"
      ? Object.keys(body.errors).slice(0, 10)
      : []
  return {
    status: action.write ? "unknown" : "failed",
    data,
    reason: failureReason(httpStatus),
    httpStatus,
    message:
      typeof body.message === "string" && body.message.trim()
        ? body.message.slice(0, 200)
        : "The request was refused.",
    ...(fields.length ? { fields } : {}),
  } satisfies Outcome
}

function completed(action: PreparedAction, data: unknown): Outcome {
  const links = recordLinks(action, data)
  const view = action.write ? undefined : resultView(action.operationId, data)
  return {
    status: "completed",
    data,
    ...(links.length ? { links } : {}),
    ...(view ? { view } : {}),
  }
}

export function voiceResponse(data: unknown, status = 200) {
  return Response.json(data, {
    status,
    headers: { "Cache-Control": "no-store" },
  })
}

export function voiceFailure(error: unknown) {
  const known =
    error instanceof VoiceError || error instanceof ToolUnavailableError
      ? error
      : undefined
  const status = known?.status ?? 503
  const reason = failureReason(status)
  noteVoiceFailure(reason, status)
  return voiceResponse(
    {
      message:
        known?.message ?? "The voice service could not complete this request.",
      reason,
    },
    status
  )
}
