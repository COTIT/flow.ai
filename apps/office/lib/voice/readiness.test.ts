import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { assistantReadiness, validateNativeMultimodal } from "./readiness"

const db = vi.hoisted(() => ({
  connect: vi.fn(),
  query: vi.fn(),
  end: vi.fn(),
}))
const warm = vi.hoisted(() => vi.fn())
vi.mock("./checkpoint", () => ({ voiceCheckpointer: warm }))
vi.mock("pg", () => ({
  default: {
    Client: class {
      connect = db.connect
      query = db.query
      end = db.end
    },
  },
}))
const agent = () => ({
  agent_id: "agent-one",
  conversation_config: {
    conversation: {
      file_input: {
        enabled: true,
        max_files_in_memory: 10,
        max_files_per_conversation: 10,
      },
    },
    agent: {
      prompt: {
        llm: "native-primary",
        backup_llm_config: {
          preference: "override",
          order: ["native-backup", "native-second"],
        },
      },
    },
  },
})
const models = () => ({
  llms: ["native-primary", "native-backup", "native-second"].map((llm) => ({
    llm,
    supports_image_input: true,
    supports_document_input: true,
  })),
})

beforeEach(() => {
  Reflect.deleteProperty(
    globalThis,
    Symbol.for("genesy.office.voice.readiness.v1")
  )
  vi.stubEnv("ASSISTANT_ENABLED", "true")
  vi.stubEnv(
    "ASSISTANT_WORKSPACE_ALLOWLIST",
    "01950000-0000-7000-8000-000000000009"
  )
  vi.stubEnv("ELEVENLABS_API_KEY", "provider-private")
  vi.stubEnv("ELEVENLABS_AGENT_ID", "agent-one")
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "b".repeat(32))
  vi.stubEnv(
    "ASSISTANT_CHECKPOINT_URL",
    "postgres://restricted:private@db/test"
  )
  vi.stubEnv("API_URL", "http://backend.test/v1")
  warm.mockReset().mockResolvedValue(undefined)
  db.connect.mockResolvedValue(undefined)
  db.end.mockResolvedValue(undefined)
  db.query.mockResolvedValue({
    rows: [{ version: 5, restricted: true, checkpoint_access: true }],
  })
})
afterEach(() => {
  Reflect.deleteProperty(
    globalThis,
    Symbol.for("genesy.office.voice.readiness.v1")
  )
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
  vi.clearAllMocks()
})

it("requires image and document capabilities on every fallback, not merely the first", () => {
  expect(() => validateNativeMultimodal(agent(), models())).not.toThrow()
  const catalog = models()
  catalog.llms[2]!.supports_document_input = false
  expect(() => validateNativeMultimodal(agent(), catalog)).toThrow()
  const configured = agent()
  configured.conversation_config.agent.prompt.backup_llm_config.preference =
    "default"
  expect(() => validateNativeMultimodal(configured, models())).toThrow()
})

it("rejects disabled file input, custom primary models and unknown fallbacks", () => {
  const disabled = agent()
  disabled.conversation_config.conversation.file_input.enabled = false
  expect(() => validateNativeMultimodal(disabled, models())).toThrow()
  const custom = agent()
  custom.conversation_config.agent.prompt.llm = "custom-llm"
  expect(() => validateNativeMultimodal(custom, models())).toThrow()
  const unknown = agent()
  unknown.conversation_config.agent.prompt.backup_llm_config.order.push(
    "unknown"
  )
  expect(() => validateNativeMultimodal(unknown, models())).toThrow()
})

it("does no provider or database work while disabled", async () => {
  vi.stubEnv("ASSISTANT_ENABLED", "false")
  const fetcher = vi.fn()
  vi.stubGlobal("fetch", fetcher)
  expect(await assistantReadiness()).toEqual({ enabled: false, ready: true })
  expect(fetcher).not.toHaveBeenCalled()
  expect(db.connect).not.toHaveBeenCalled()
})

it("checks real provider config, backend liveness and restricted storage without generating a session", async () => {
  const fetcher = vi.fn(async (url: string) =>
    url.endsWith("/llm/list")
      ? Response.json(models())
      : url.includes("/agents/")
        ? Response.json(agent())
        : Response.json({ status: "ok" })
  )
  vi.stubGlobal("fetch", fetcher)
  expect(await assistantReadiness(false)).toEqual({
    enabled: true,
    ready: true,
    checks: {
      configuration: true,
      provider: true,
      checkpoint: true,
      backend: true,
    },
  })
  expect(fetcher.mock.calls.map(([url]) => url)).toEqual(
    expect.arrayContaining([
      "http://backend.test/healthz",
      "https://api.elevenlabs.io/v1/convai/llm/list",
      "https://api.elevenlabs.io/v1/convai/agents/agent-one",
    ])
  )
  expect(db.end).toHaveBeenCalledOnce()
})

it("sanitizes failures and rejects an overprivileged checkpoint role", async () => {
  db.query.mockResolvedValue({
    rows: [{ version: 5, restricted: false, checkpoint_access: true }],
  })
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      throw new Error("provider-private postgres://restricted:private@db/test")
    })
  )
  const result = await assistantReadiness(false)
  expect(result.ready).toBe(false)
  expect(JSON.stringify(result)).not.toMatch(
    /provider-private|postgres|private|agent-one/
  )
  expect(result.checks).toMatchObject({
    checkpoint: false,
    provider: false,
    backend: false,
  })
})

it("blocks readiness for an unsupported fallback without exposing provider details", async () => {
  const catalog = models()
  catalog.llms[2]!.supports_image_input = false
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      Response.json(url.endsWith("/llm/list") ? catalog : agent())
    )
  )
  const result = await assistantReadiness(false)
  expect(result.ready).toBe(false)
  expect(result.checks?.provider).toBe(false)
})

it("shares a short successful readiness check across concurrent session starts", async () => {
  vi.stubEnv("ELEVENLABS_AGENT_ID", "agent-cache")
  const configured = { ...agent(), agent_id: "agent-cache" }
  const fetcher = vi.fn(async (url: string) =>
    Response.json(url.endsWith("/llm/list") ? models() : configured)
  )
  vi.stubGlobal("fetch", fetcher)
  await Promise.all([assistantReadiness(), assistantReadiness()])
  await assistantReadiness()
  expect(fetcher).toHaveBeenCalledTimes(3)
  expect(db.connect).toHaveBeenCalledOnce()
})

it("shares in-flight probes and cached results across separately loaded route bundles", async () => {
  let connected!: () => void
  db.connect.mockReturnValue(
    new Promise<void>((resolve) => {
      connected = resolve
    })
  )
  const fetcher = vi.fn(async (url: string) =>
    Response.json(url.endsWith("/llm/list") ? models() : agent())
  )
  vi.stubGlobal("fetch", fetcher)
  vi.resetModules()
  const first = await import("./readiness")
  const firstProbe = first.assistantReadiness()
  vi.resetModules()
  const second = await import("./readiness")
  const secondProbe = second.assistantReadiness()
  connected()
  expect(
    (await Promise.all([firstProbe, secondProbe])).every(
      (result) => result.ready
    )
  ).toBe(true)
  await second.assistantReadiness()
  expect(db.connect).toHaveBeenCalledOnce()
  expect(fetcher).toHaveBeenCalledTimes(3)
  const cached = Reflect.get(
    globalThis,
    Symbol.for("genesy.office.voice.readiness.v1")
  )
  expect(JSON.stringify(cached)).not.toMatch(
    /provider-private|postgres|restricted:private/
  )
})

it("pre-connects the shared checkpoint pool once storage is ready", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string) =>
      url.endsWith("/llm/list")
        ? Response.json(models())
        : url.includes("/agents/")
          ? Response.json(agent())
          : Response.json({ status: "ok" })
    )
  )
  warm.mockRejectedValueOnce(new Error("private pool failure"))
  expect((await assistantReadiness(false)).ready).toBe(true)
  expect(warm).toHaveBeenCalledOnce()
  db.query.mockResolvedValue({
    rows: [{ version: 5, restricted: false, checkpoint_access: true }],
  })
  warm.mockClear()
  expect((await assistantReadiness(false)).ready).toBe(false)
  expect(warm).not.toHaveBeenCalled()
})
