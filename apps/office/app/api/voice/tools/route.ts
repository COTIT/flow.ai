import { randomUUID } from "node:crypto"
import {
  discoverTools,
  getOperation,
  isToolAllowed,
  prepareAction,
  tools,
  ToolUnavailableError,
} from "@/lib/voice/catalog"
import {
  authorizeVoice,
  VoiceError,
  voiceFailure,
  voiceResponse,
} from "@/lib/voice/server"
import {
  assistantRequest,
  readAssistantSession,
  requireAssistantSession,
} from "@/lib/voice/bridge"
import {
  actionStatus,
  proposeAction,
  restoreReview,
  type StoredAction,
} from "@/lib/voice/actions"
import { executeAction } from "@/lib/voice/workflow"
import { takeFilePermit, voiceInput } from "@/lib/voice/input"
import { requireAssistantAdmission } from "@/lib/voice/admission"
import {
  batchFailure,
  proposeBatch,
  batchStatus,
  type StoredBatch,
  BATCH_PREPARE_OPERATION,
  BATCH_STATUS_OPERATION,
} from "@/lib/voice/batches"
export const runtime = "nodejs"

const statusTool = {
  id: "assistant_task_status",
  summary:
    "Check this session's recorded task outcomes; never retry unknown writes. After presenting a review, wait for its confirmed outcome with wait:true.",
  method: "GET",
  write: false,
  body: {
    type: "object",
    properties: {
      actionId: { type: "string", format: "uuid" },
      wait: { type: "boolean" },
    },
  },
}
const batchPrepareTool = {
  id: BATCH_PREPARE_OPERATION,
  summary:
    "Propose one fixed ordered batch of two through ten fully resolved writes. One independent Office confirmation covers only these exact requests. Any failure, unknown, queued or partial result stops the remainder; no dynamic result references.",
  method: "POST",
  write: true,
  body: {
    type: "object",
    required: ["calls"],
    properties: {
      calls: {
        type: "array",
        minItems: 2,
        maxItems: 10,
        items: {
          type: "object",
          required: ["operationId", "arguments"],
          properties: {
            operationId: { type: "string" },
            arguments: { type: "object" },
          },
        },
      },
    },
  },
}
const batchStatusTool = {
  id: BATCH_STATUS_OPERATION,
  summary:
    "Read an owned fixed batch's recorded steps; never retry or resume unknown/paused writes. Use wait:true while independent Office confirmation is pending.",
  method: "GET",
  write: false,
  body: {
    type: "object",
    properties: {
      batchId: { type: "string", format: "uuid" },
      wait: { type: "boolean" },
    },
  },
}
/** Model bridge can propose operations and read status; it cannot approve or commit. */
export async function POST(request: Request) {
  let releaseFile: (() => void) | undefined
  let language = "en"
  let isBatch = false
  try {
    releaseFile = takeFilePermit(request)
    const { input, files } = await voiceInput(request)
    const statusRequest =
      input.command === "status" ||
      (input.command === "execute" &&
        (input.operationId === BATCH_STATUS_OPERATION ||
          input.operationId === statusTool.id))
    if (!statusRequest) await requireAssistantAdmission()
    const context = await authorizeVoice(request, input.workspaceId)
    if (!statusRequest) await requireAssistantAdmission(context.workspaceId)
    const session = statusRequest
      ? await readAssistantSession(context, input.sessionId)
      : await requireAssistantSession(context, input.sessionId)
    if (!statusRequest && !session.conversation_id)
      throw new VoiceError("Conversation identity has not been bound.", 409)
    context.sessionId = session.id
    context.language = session.language ?? "en"
    language = context.language
    isBatch =
      input.operationId === BATCH_PREPARE_OPERATION ||
      input.operationId === BATCH_STATUS_OPERATION
    if (input.command === "discover") {
      if (files.size) throw new VoiceError("Discovery does not accept files.")
      const query = String(input.query ?? "")
      return voiceResponse({
        tools:
          query === BATCH_PREPARE_OPERATION
            ? [batchPrepareTool]
            : query === BATCH_STATUS_OPERATION
              ? [batchStatusTool]
              : query === statusTool.id
                ? [statusTool]
                : [
                    ...discoverTools(query, context.permissions),
                    ...(/batch|ordered|bulk|several/i.test(query)
                      ? [batchPrepareTool, batchStatusTool]
                      : []),
                    ...(/status|task|pending|last/i.test(query)
                      ? [statusTool]
                      : []),
                  ],
        domains: [
          ...new Set(
            tools
              .filter((tool) => isToolAllowed(tool, context.permissions))
              .map((tool) => tool.domain)
          ),
        ],
      })
    }
    const argumentsValue =
      input.arguments &&
      typeof input.arguments === "object" &&
      !Array.isArray(input.arguments)
        ? (input.arguments as Record<string, unknown>)
        : {}
    if (
      input.command === "execute" &&
      input.operationId === BATCH_PREPARE_OPERATION
    )
      return voiceResponse(
        await proposeBatch(argumentsValue.calls, context, files)
      )
    if (
      input.command === "execute" &&
      input.operationId === BATCH_STATUS_OPERATION
    ) {
      if (files.size)
        throw new VoiceError("Batch status does not accept files.")
      const batchId = argumentsValue.batchId
      if (batchId) {
        if (typeof batchId !== "string" || !/^[0-9a-f-]{36}$/i.test(batchId))
          throw new VoiceError("Invalid fixed batch reference.")
        const stored = await assistantRequest<StoredBatch>(
          context,
          `/batches/${batchId}`
        )
        if (stored.session_id !== session.id)
          throw new VoiceError(
            "This fixed batch belongs to another session.",
            403
          )
        return voiceResponse(batchStatus(stored, context.language))
      }
      const rows = await assistantRequest<StoredBatch[]>(
        context,
        `/sessions/${session.id}/batches`
      )
      return voiceResponse({
        status: "completed",
        data: rows.map((row) => batchStatus(row, context.language)),
      })
    }
    if (
      input.command === "status" ||
      input.command === "restore" ||
      (input.command === "execute" && input.operationId === statusTool.id)
    ) {
      if (files.size) throw new VoiceError("Task status does not accept files.")
      const actionId = input.actionId ?? argumentsValue.actionId
      if (actionId) {
        if (typeof actionId !== "string" || !/^[0-9a-f-]{36}$/i.test(actionId))
          throw new VoiceError("Invalid action reference.")
        const stored = await assistantRequest<StoredAction>(
          context,
          "/actions/" + actionId
        )
        if (stored.session_id !== session.id)
          throw new VoiceError("This action belongs to another session.", 403)
        if (input.command === "restore") {
          const operation = getOperation(stored.operation_id)
          if (!operation || !isToolAllowed(operation, context.permissions))
            throw new VoiceError("This action is no longer permitted.", 403)
          return voiceResponse(await restoreReview(stored, context))
        }
        return voiceResponse(actionStatus(stored, session.state === "active"))
      }
      if (input.command === "restore")
        throw new VoiceError("Choose a task to review.")
      const history = await assistantRequest<StoredAction[]>(
        context,
        "/sessions/" + session.id + "/actions"
      )
      const batches = await assistantRequest<StoredBatch[]>(
        context,
        "/sessions/" + session.id + "/batches"
      )
      return voiceResponse({
        status: "completed",
        data: [
          ...history
            .filter((stored) => !stored.batch_id)
            .map((stored) => actionStatus(stored, session.state === "active")),
          ...batches.map((row) => batchStatus(row, context.language)),
        ],
      })
    }
    if (input.command !== "execute")
      throw new VoiceError("Unknown assistant command.")
    const operation = getOperation(String(input.operationId ?? ""))
    if (!operation || !["execute", "secure_ui"].includes(operation.disposition))
      throw new VoiceError("This operation is not exposed to the assistant.")
    if (!isToolAllowed(operation, context.permissions))
      throw new VoiceError("You do not have permission for this action.", 403)
    if (operation.disposition === "secure_ui") {
      const route = operation.handoff?.route
      if (!route || !route.startsWith("/") || route.startsWith("//"))
        throw new VoiceError("The secure Office form is not available.", 503)
      return voiceResponse({
        status: "handoff",
        message:
          "Open the secure Office form. The conversation will stop before confidential input.",
        handoff: { route, operationId: operation.id },
      })
    }
    let action
    try {
      action = prepareAction(
        operation.id,
        input.arguments ?? {},
        context.workspaceId
      )
    } catch (error) {
      if (error instanceof ToolUnavailableError) throw error
      throw new VoiceError(
        error instanceof Error ? error.message : "Invalid arguments."
      )
    }
    if (action.write)
      return voiceResponse(await proposeAction(action, context, files))
    if (files.size)
      throw new VoiceError("This lookup does not accept binary attachments.")
    return voiceResponse(await executeAction(action, context, randomUUID()))
  } catch (error) {
    return isBatch ? batchFailure(error, language) : voiceFailure(error)
  } finally {
    releaseFile?.()
  }
}
