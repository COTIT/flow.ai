import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { readFile } from "node:fs/promises"

vi.mock("node:fs/promises", () => ({ readFile: vi.fn() }))

beforeEach(() =>
  Reflect.deleteProperty(
    globalThis,
    Symbol.for("genesy.office.voice.tracing.v1")
  )
)
afterEach(() => {
  Reflect.deleteProperty(
    globalThis,
    Symbol.for("genesy.office.voice.tracing.v1")
  )
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
})

it("initializes a runtime-file tracing key, hides payloads and bounds exporter outages", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "true")
  vi.stubEnv("LANGSMITH_API_KEY", "")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "/run/secrets/tracing")
  vi.mocked(readFile).mockResolvedValue("trace-private\n")
  vi.stubEnv("LANGSMITH_HIDE_INPUTS", "false")
  vi.stubEnv("LANGSMITH_HIDE_OUTPUTS", "false")
  vi.spyOn(console, "error").mockImplementation(() => {})
  vi.spyOn(console, "warn").mockImplementation(() => {})
  const fetcher = vi.fn(async () => {
    throw new Error("trace exporter unavailable")
  })
  vi.stubGlobal("fetch", fetcher)
  const { withVoiceTracing } = await import("./tracing")
  const action = vi.fn(async () => ({ business: "private-business-result" }))
  expect(await withVoiceTracing("assistant_test", action)).toEqual({
    business: "private-business-result",
  })
  expect(action).toHaveBeenCalledOnce()
  expect(process.env.LANGSMITH_HIDE_INPUTS).toBe("true")
  expect(process.env.LANGSMITH_HIDE_OUTPUTS).toBe("true")
  expect(process.env.LANGCHAIN_CALLBACKS_BACKGROUND).toBe("false")
  expect(readFile).toHaveBeenCalledWith("/run/secrets/tracing", "utf8")
  expect(fetcher).toHaveBeenCalled()
  expect(fetcher.mock.calls.length).toBeLessThanOrEqual(3)
  const calls = fetcher.mock.calls.length
  vi.resetModules()
  const laterBundle = await import("./tracing")
  expect(
    await laterBundle.withVoiceTracing("assistant_later", async () => "done")
  ).toBe("done")
  expect(fetcher.mock.calls.length).toBe(calls)
})

it("does not retry a failed business action when tracing also fails", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "false")
  const { withVoiceTracing } = await import("./tracing")
  const action = vi.fn(async () => {
    throw new Error("business refused")
  })
  await expect(withVoiceTracing("assistant_test", action)).rejects.toThrow(
    "business refused"
  )
  expect(action).toHaveBeenCalledOnce()
})

it("redacts a failed graph node's error while preserving the original once-only failure", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "true")
  vi.stubEnv("LANGSMITH_API_KEY", "trace-private")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "")
  const bodies: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (_url, options) => {
      if (typeof options?.body === "string") bodies.push(options.body)
      else if (options?.body instanceof Uint8Array)
        bodies.push(new TextDecoder().decode(options.body))
      return new Response(null, { status: 200 })
    })
  )
  const { withVoiceTracing } = await import("./tracing")
  const { Annotation, StateGraph, START, END } =
    await import("@langchain/langgraph")
  const state = Annotation.Root({ status: Annotation<string>() })
  const execute = vi.fn(async () => {
    throw new Error("private-business-error secret-token attachment-name.pdf")
  })
  const graph = new StateGraph(state)
    .addNode("execute_backend", execute)
    .addEdge(START, "execute_backend")
    .addEdge("execute_backend", END)
    .compile()
  await expect(
    withVoiceTracing("assistant_failed_graph", () =>
      graph.invoke({ status: "private-input" })
    )
  ).rejects.toThrow("private-business-error")
  expect(execute).toHaveBeenCalledOnce()
  await vi.waitFor(() => expect(bodies.join("\n")).toContain('"end_time"'))
  expect(bodies.join("\n")).not.toMatch(
    /private-business-error|secret-token|attachment-name|private-input|trace-private/
  )
})

it("preserves graph node traces through the bounded client while hiding business results", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "true")
  vi.stubEnv("LANGSMITH_API_KEY", "trace-private")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "")
  const bodies: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (_url, options) => {
      if (typeof options?.body === "string") bodies.push(options.body)
      else if (options?.body instanceof Uint8Array)
        bodies.push(new TextDecoder().decode(options.body))
      return new Response(null, { status: 200 })
    })
  )
  const { withVoiceTracing } = await import("./tracing")
  const { Annotation, StateGraph, START, END } =
    await import("@langchain/langgraph")
  const state = Annotation.Root({ status: Annotation<string>() })
  const graph = new StateGraph(state)
    .addNode("execute_backend", async () => ({
      status: "private-business-result",
    }))
    .addEdge(START, "execute_backend")
    .addEdge("execute_backend", END)
    .compile()
  expect(
    await withVoiceTracing("assistant_graph_test", () =>
      graph.invoke({ status: "private-business-input" })
    )
  ).toEqual({ status: "private-business-result" })
  expect(bodies.join("\n")).toContain("execute_backend")
  expect(bodies.join("\n")).not.toMatch(/private-business-(input|result)/)
})

it("reuses initialization and the SDK client across route bundles after implicit tracing is disabled", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "true")
  vi.stubEnv("LANGSMITH_API_KEY", "")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "/run/secrets/tracing")
  vi.mocked(readFile).mockClear().mockResolvedValue("trace-private\n")
  const bodies: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (_url, options) => {
      if (typeof options?.body === "string") bodies.push(options.body)
      else if (options?.body instanceof Uint8Array)
        bodies.push(new TextDecoder().decode(options.body))
      return new Response(null, { status: 200 })
    })
  )
  const first = await import("./tracing")
  await first.withVoiceTracing(
    "assistant_first_bundle",
    async () => "private-first-result"
  )
  expect(process.env.LANGSMITH_TRACING).toBe("false")
  vi.resetModules()
  const second = await import("./tracing")
  await second.withVoiceTracing(
    "assistant_second_bundle",
    async () => "private-second-result"
  )
  expect(bodies.join("\n")).toContain("assistant_first_bundle")
  expect(bodies.join("\n")).toContain("assistant_second_bundle")
  expect(bodies.join("\n")).not.toMatch(
    /private-(first|second)-result|trace-private/
  )
  expect(
    vi
      .mocked(readFile)
      .mock.calls.filter(([path]) => path === "/run/secrets/tracing")
  ).toHaveLength(1)
})

it("exports only opaque action references and verified status alongside hidden business payloads", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "true")
  vi.stubEnv("LANGSMITH_API_KEY", "trace-private")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "")
  const bodies: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (_url, options) => {
      if (typeof options?.body === "string") bodies.push(options.body)
      else if (options?.body instanceof Uint8Array)
        bodies.push(new TextDecoder().decode(options.body))
      return new Response(null, { status: 200 })
    })
  )
  const { withVoiceTracing } = await import("./tracing")
  await withVoiceTracing(
    "assistant_metadata",
    async () => ({ status: "completed", private: "private-business-result" }),
    {
      operation: "orders_create",
      actionId: "01950000-0000-7000-8000-000000000001",
      sessionId: "01950000-0000-7000-8000-000000000002",
    }
  )
  await vi.waitFor(() =>
    expect(bodies.join("\n")).toContain('"verified_status":"completed"')
  )
  const exported = bodies.join("\n")
  expect(exported).toContain('"operation":"orders_create"')
  expect(exported).toContain("01950000-0000-7000-8000-000000000001")
  expect(exported).not.toMatch(/private-business-result|trace-private/)
})

it("labels failures for monitoring without exporting the refused data", async () => {
  vi.resetModules()
  vi.stubEnv("LANGSMITH_TRACING", "true")
  vi.stubEnv("LANGSMITH_API_KEY", "trace-private")
  vi.stubEnv("LANGSMITH_API_KEY_FILE", "")
  const bodies: string[] = []
  vi.stubGlobal(
    "fetch",
    vi.fn<typeof fetch>(async (_url, options) => {
      if (typeof options?.body === "string") bodies.push(options.body)
      else if (options?.body instanceof Uint8Array)
        bodies.push(new TextDecoder().decode(options.body))
      return new Response(null, { status: 200 })
    })
  )
  const { noteVoiceFailure, withVoiceTracing } = await import("./tracing")
  await withVoiceTracing("assistant_refused", async () => ({
    status: "failed",
    reason: "invalid",
    httpStatus: 422,
    message: "private-refusal-detail",
  }))
  await withVoiceTracing("assistant_discover", async () => {
    noteVoiceFailure("unsupported", undefined, "  book a\n delivery truck ")
    return null
  })
  await vi.waitFor(() =>
    expect(bodies.join("\n")).toContain(
      '"unsupported_query":"book a delivery truck"'
    )
  )
  const exported = bodies.join("\n")
  expect(exported).toContain('"failure_reason":"invalid"')
  expect(exported).toContain('"failure_category":"user_fixable"')
  expect(exported).toContain('"http_status":422')
  expect(exported).toContain('"failure_category":"out_of_scope_candidate"')
  expect(exported).not.toContain("private-refusal-detail")
})
