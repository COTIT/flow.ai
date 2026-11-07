import "server-only"
import { Client } from "langsmith"
import { getCurrentRunTree, traceable } from "langsmith/traceable"
import { voiceSetting } from "./settings"

type TracingRuntime = {
  initialization?: Promise<void>
  client?: Client
  project: string
  unavailableUntil: number
}
const runtimeKey = Symbol.for("genesy.office.voice.tracing.v1")
function tracingRuntime(): TracingRuntime {
  const shared = globalThis as typeof globalThis & {
    [runtimeKey]?: TracingRuntime
  }
  return (shared[runtimeKey] ??= {
    project: "genesy-office-assistant",
    unavailableUntil: 0,
  })
}

/** Route bundles reuse one bounded SDK client and its original tracing configuration. */
export async function initializeVoiceTracing(): Promise<void> {
  const state = tracingRuntime()
  state.initialization ??= (async () => {
    const enabled = process.env.LANGSMITH_TRACING === "true"
    process.env.LANGSMITH_HIDE_INPUTS = "true"
    process.env.LANGSMITH_HIDE_OUTPUTS = "true"
    process.env.LANGCHAIN_CALLBACKS_BACKGROUND = "false"
    // LangGraph inherits the wrapper's bounded client; implicit clients remain disabled.
    process.env.LANGSMITH_TRACING = "false"
    process.env.LANGCHAIN_TRACING_V2 = "false"
    process.env.LANGCHAIN_TRACING = "false"
    if (!enabled) return
    try {
      const key = await voiceSetting("LANGSMITH_API_KEY")
      if (!key) return
      state.project = (await voiceSetting("LANGSMITH_PROJECT")) || state.project
      state.client = new Client({
        apiKey: key,
        hideInputs: true,
        hideOutputs: true,
        omitTracedRuntimeInfo: true,
        // The SDK falls back to the original error unless the replacement is a string.
        anonymizer: (values) =>
          "error" in values ? { error: "Assistant task failed." } : values,
        autoBatchTracing: false,
        timeout_ms: 500,
        maxIngestMemoryBytes: 131072,
        callerOptions: {
          maxRetries: 0,
          maxConcurrency: 2,
          maxQueueSizeBytes: 65536,
        },
        fetchImplementation: async (...args) => {
          if (Date.now() < state.unavailableUntil)
            return new Response(null, { status: 503 })
          try {
            const response = await fetch(...args)
            if (!response.ok) state.unavailableUntil = Date.now() + 5000
            return response
          } catch {
            state.unavailableUntil = Date.now() + 5000
            return new Response(null, { status: 503 })
          }
        },
      })
    } catch {
      /* Optional tracing never blocks business work or exposes configuration errors. */
    }
  })()
  await state.initialization
}

/** Memoize the action promise so an exporter failure can never replay a business action. */
export async function withVoiceTracing<T>(
  name: string,
  action: () => Promise<T>,
  references: { operation?: string; actionId?: string; sessionId?: string } = {}
): Promise<T> {
  await initializeVoiceTracing()
  const state = tracingRuntime()
  let result: Promise<T> | undefined
  const run = () => (result ??= Promise.resolve().then(action))
  if (!state.client) return run()
  const metadata: Record<string, string> = {}
  if (
    references.operation &&
    /^[a-z][a-z0-9_]{1,99}$/.test(references.operation)
  )
    metadata.operation = references.operation
  for (const [key, value] of [
    ["action_id", references.actionId],
    ["session_id", references.sessionId],
  ])
    if (
      value &&
      /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
        value
      )
    )
      metadata[key!] = value
  const traced = async () => {
    const value = await run()
    if (
      value &&
      typeof value === "object" &&
      "status" in value &&
      typeof value.status === "string" &&
      [
        "completed",
        "failed",
        "unknown",
        "cancelled",
        "partial",
        "queued",
      ].includes(value.status)
    ) {
      try {
        const tree = getCurrentRunTree()
        tree.metadata = { ...tree.metadata, verified_status: value.status }
      } catch {
        /* Status export is optional; the action result remains authoritative. */
      }
    }
    return value
  }
  try {
    return await traceable(traced, {
      name: /^[a-z][a-z0-9_]{1,80}$/.test(name) ? name : "assistant_task",
      run_type: "chain",
      client: state.client,
      tracingEnabled: true,
      project_name: state.project,
      processInputs: () => ({}),
      processOutputs: () => ({}),
      metadata,
    })()
  } catch {
    return run()
  }
}
