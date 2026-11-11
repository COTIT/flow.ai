import "server-only"
import { AsyncLocalStorage } from "node:async_hooks"
import { createHash, createHmac, timingSafeEqual } from "node:crypto"
import {
  Annotation,
  Command,
  END,
  START,
  StateGraph,
  interrupt,
} from "@langchain/langgraph"
import type { PostgresSaver } from "@langchain/langgraph-checkpoint-postgres"
import type { PreparedAction } from "./catalog"
import type { MultipartReview } from "./multipart"
import {
  callBackend,
  VoiceError,
  type Outcome,
  type VoiceContext,
} from "./server"
import { voiceCheckpointer, withVoiceThread } from "./checkpoint"
import { withVoiceTracing } from "./tracing"

type Review = {
  action: PreparedAction
  owner: string
  workspace: string
  session: string
  id: string
  expires: number
  confirmationCode: string
  multipart?: MultipartReview
}
const noPendingReview =
  "This action has no pending review. Check its recorded status."
const digest = (s: string) => createHash("sha256").update(s).digest("hex")

/** Signed UI review contains no bearer and binds the backend's durable action identity. */
export function createReview(
  action: PreparedAction,
  context: VoiceContext,
  secret: string,
  sessionId: string,
  actionId: string,
  expiresAt = new Date(Date.now() + 120000).toISOString(),
  multipart?: MultipartReview
): string {
  const confirmationCode = String(
    createHmac("sha256", secret)
      .update(`approval-code\n${sessionId}\n${actionId}\n${expiresAt}`)
      .digest()
      .readUInt32BE(0) % 1000000
  ).padStart(6, "0")
  const review: Review = {
    action,
    owner: digest(context.token),
    workspace: context.workspaceId,
    session: sessionId,
    id: actionId,
    expires: Date.parse(expiresAt),
    confirmationCode,
    ...(multipart ? { multipart } : {}),
  }
  const payload = Buffer.from(JSON.stringify(review)).toString("base64url")
  return (
    payload +
    "." +
    createHmac("sha256", secret).update(payload).digest("base64url")
  )
}

export function verifyReview(
  ticket: string,
  context: VoiceContext,
  secret: string
): Review {
  const [payload, signature, extra] = ticket.split(".")
  if (!payload || !signature || extra)
    throw new VoiceError("Invalid action review.")
  const expected = createHmac("sha256", secret).update(payload).digest()
  const given = Buffer.from(signature, "base64url")
  if (given.length !== expected.length || !timingSafeEqual(given, expected))
    throw new VoiceError("Invalid action review.")
  const review = JSON.parse(
    Buffer.from(payload, "base64url").toString()
  ) as Review
  if (
    review.owner !== digest(context.token) ||
    review.workspace !== context.workspaceId ||
    review.session !== context.sessionId ||
    !Number.isFinite(review.expires) ||
    review.expires < Date.now()
  )
    throw new VoiceError("This review expired or belongs to another session.")
  return review
}

type Execution = {
  action: PreparedAction
  context: VoiceContext
  actionId: string
  backend: typeof callBackend
  outcome?: Outcome
}
/** Private request context never enters graph state, configurable metadata or exported traces. */
const execution = new AsyncLocalStorage<Execution>()
const State = Annotation.Root({
  operation: Annotation<string>(),
  actionId: Annotation<string>(),
  status: Annotation<string>(),
  steps: Annotation<string[]>({
    reducer: (previous, next) => [...new Set([...previous, ...next])],
    default: () => [],
  }),
})
function buildGraph(checkpointer: PostgresSaver) {
  return new StateGraph(State)
    .addNode("await_review", (state) => {
      const decision = interrupt({
        actionId: state.actionId,
        operation: state.operation,
      })
      // Resume carries the reviewed action ID; anything else never approves.
      return {
        status: decision === state.actionId ? "approved" : "cancelled",
      }
    })
    .addNode("execute_backend", async (state) => {
      const scope = execution.getStore()
      if (!scope || scope.actionId !== state.actionId)
        throw new VoiceError("Task execution context does not match.", 409)
      try {
        scope.outcome = await scope.backend(
          scope.action,
          scope.context,
          scope.actionId
        )
      } catch {
        scope.outcome = {
          status: "unknown",
          actionId: scope.actionId,
          message:
            "Completion could not be verified. Check the recorded action before repeating it.",
        }
      }
      return { status: scope.outcome.status }
    })
    .addEdge(START, "await_review")
    .addConditionalEdges("await_review", (state) =>
      state.status === "approved" ? "execute_backend" : END
    )
    .addEdge("execute_backend", END)
    .compile({ checkpointer })
}
let graph: ReturnType<typeof buildGraph> | undefined
async function taskGraph() {
  return (graph ??= buildGraph(await voiceCheckpointer()))
}
function config(context: VoiceContext) {
  if (!context.sessionId)
    throw new VoiceError("Start an assistant session first.")
  return {
    configurable: { thread_id: context.sessionId },
    runName: "genesy_assistant_task",
    recursionLimit: 12,
    metadata: { workflow_version: 2, batch_version: 1 },
  }
}

/** Refuse a new proposal before its ledger row exists; checkpointReview still rechecks (race). */
export async function assertReviewFree(context: VoiceContext) {
  const settings = config(context)
  await withVoiceThread(context.sessionId!, async () => {
    const current = await withVoiceTracing("checkpoint_state", async () =>
      (await taskGraph()).getState(settings)
    )
    if (current.tasks.some((task) => task.interrupts?.length))
      throw new VoiceError("Finish or cancel the pending review first.", 409)
  })
}

/** Persist approval interrupt; no backend business command runs here. */
export async function checkpointReview(
  action: PreparedAction,
  context: VoiceContext,
  actionId: string,
  stepIds: string[] = [actionId]
) {
  return withVoiceTracing(
    "genesy_assistant_review",
    async () => {
      const settings = config(context)
      await withVoiceThread(context.sessionId!, async () => {
        const workflow = await taskGraph()
        const current = await withVoiceTracing("checkpoint_state", () =>
          workflow.getState(settings)
        )
        if (
          current.tasks.some((task) => task.interrupts?.length) &&
          current.values.actionId !== actionId
        )
          throw new VoiceError(
            "Finish or cancel the pending review first.",
            409
          )
        if (current.tasks.some((task) => task.interrupts?.length)) return
        await workflow.invoke(
          {
            operation: action.operationId,
            actionId,
            status: "awaiting_review",
            steps: stepIds,
          },
          settings
        )
      })
    },
    { operation: action.operationId, actionId, sessionId: context.sessionId }
  )
}

export async function cancelReview(context: VoiceContext, actionId: string) {
  return withVoiceTracing(
    "genesy_assistant_cancel",
    async () => {
      const settings = config(context)
      await withVoiceThread(context.sessionId!, async () => {
        const workflow = await taskGraph()
        const state = await withVoiceTracing("checkpoint_state", () =>
          workflow.getState(settings)
        )
        if (
          state.values.actionId === actionId &&
          state.tasks.some((task) => task.interrupts?.length)
        )
          await workflow.invoke(new Command({ resume: false }), settings)
      })
    },
    { actionId, sessionId: context.sessionId }
  )
}

/** Resume only matching persisted review; backend ledger independently consumes approval once. */
export async function executeAction(
  action: PreparedAction,
  context: VoiceContext,
  actionId: string,
  backend = callBackend
): Promise<Outcome> {
  return withVoiceTracing(
    "genesy_assistant_execute",
    async () => {
      const scope: Execution = { action, context, actionId, backend }
      if (!action.write) {
        try {
          scope.outcome = await backend(action, context, actionId)
        } catch {
          return {
            status: "failed",
            actionId,
            message: "The backend could not complete this lookup.",
          }
        }
        return { ...scope.outcome!, actionId }
      }
      const settings = config(context)
      return withVoiceThread(context.sessionId!, async () => {
        const workflow = await taskGraph()
        // Kept: a failed resume leaves its value pending, so mismatches must be refused before invoke.
        const current = await withVoiceTracing("checkpoint_state", () =>
          workflow.getState(settings)
        )
        if (
          current.values.actionId !== actionId ||
          !current.tasks.some((task) => task.interrupts?.length)
        )
          throw new VoiceError(noPendingReview, 409)
        await execution.run(scope, () =>
          workflow.invoke(new Command({ resume: actionId }), {
            ...settings,
            durability: "async",
          })
        )
        if (!scope.outcome) throw new VoiceError(noPendingReview, 409)
        return { ...scope.outcome, actionId }
      })
    },
    { operation: action.operationId, actionId, sessionId: context.sessionId }
  )
}
