// @vitest-environment jsdom
import { act, useEffect, type ReactNode } from "react"
import type { useConversation } from "@elevenlabs/react"
import type { HistoryEvent } from "../lib/history"
import { createRoot, type Root } from "react-dom/client"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import {
  GenesyVoiceProvider,
  useVoice,
  type VoiceContextValue,
} from "./provider"

type SdkOptions = NonNullable<Parameters<typeof useConversation>[0]>
type WireBody = {
  history?: boolean
  logicalConversationId?: string
  command?: string
  events: HistoryEvent[]
}

const login = vi.hoisted(() => ({
  workspaceId: "w1" as string | null,
  token: "t1" as string | null,
  signOut: vi.fn(),
}))

const sdk = vi.hoisted(() => ({
  sessions: [] as SdkOptions[],
  hook: {} as SdkOptions,
  startSession: vi.fn(),
  endSession: vi.fn(),
  sendUserMessage: vi.fn(),
  setMuted: vi.fn(),
  changeInputDevice: vi.fn(async () => {}),
  sendContextualUpdate: vi.fn(),
  sendUserActivity: vi.fn(),
}))
vi.mock("@elevenlabs/react", () => ({
  ConversationProvider: ({ children }: { children: ReactNode }) => children,
  useRawConversation: () => ({ endSession: async () => {} }),
  useConversation: (options: SdkOptions) => {
    sdk.hook = options
    return {
      ...sdk,
      status: "connected",
      isSpeaking: false,
      isMuted: false,
      getInputVolume: () => 0,
      getOutputVolume: () => 0,
    }
  },
}))
vi.mock("next-intl", () => ({
  useLocale: () => "en",
  useTranslations: () =>
    Object.assign((key: string) => key, { has: () => false }),
}))
vi.mock("@tanstack/react-query", () => ({
  useQueryClient: () => ({ invalidateQueries: vi.fn() }),
}))
vi.mock("@genesy/api-client/auth", () => ({
  useSession: (pick: (s: typeof login) => unknown) => pick(login),
}))
vi.mock("@genesy/api-client/session", () => ({
  sessionStore: { getState: () => login },
}))

let voice: VoiceContextValue
let root: Root
let host: HTMLDivElement
let calls: { path: string; body: WireBody }[]
let failEvents = false
let failDelete = false
let issueReview = false
let queuedWrite: Promise<void> | null = null
let sessionNumber = 0
let historyReply: Promise<Response> | null = null
let attachmentReply: Promise<Response> | null = null
function Probe() {
  const value = useVoice()
  useEffect(() => {
    voice = value
  }, [value])
  return null
}
async function mount(historyEnabled = true, onUnauthorized?: () => void) {
  host = document.createElement("div")
  document.body.append(host)
  root = createRoot(host)
  await act(async () =>
    root.render(
      <GenesyVoiceProvider
        historyEnabled={historyEnabled}
        onUnauthorized={onUnauthorized}
      >
        <Probe />
      </GenesyVoiceProvider>
    )
  )
}
async function start(onlyText = true) {
  await act(async () => voice.start(onlyText))
  const options = { ...sdk.hook, ...sdk.sessions.at(-1)! }
  await act(async () => options.onConnect!({ conversationId: "transport-1" }))
  return options
}
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true)
  calls = []
  failEvents = false
  failDelete = false
  issueReview = false
  queuedWrite = null
  sessionNumber = 0
  historyReply = null
  attachmentReply = null
  login.token = "t1"
  login.workspaceId = "w1"
  Object.defineProperty(navigator, "mediaDevices", {
    configurable: true,
    value: undefined,
  })
  sdk.sessions = []
  vi.clearAllMocks()
  sdk.endSession.mockImplementation(() =>
    sdk.sessions.at(-1)?.onStatusChange?.({ status: "disconnected" })
  )
  sdk.startSession.mockImplementation((options: SdkOptions) => {
    const hook = sdk.hook
    // ElevenLabs composes hook callbacks with per-start callbacks.
    sdk.sessions.push({
      ...options,
      onConnect: (event) => {
        hook.onConnect?.(event)
        options.onConnect?.(event)
      },
      onMessage: (event) => {
        hook.onMessage?.(event)
        options.onMessage?.(event)
      },
      onAgentResponseCorrection: (event) => {
        hook.onAgentResponseCorrection?.(event)
        options.onAgentResponseCorrection?.(event)
      },
    })
  })
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init: RequestInit) => {
      const body: WireBody =
        typeof init.body === "string" ? JSON.parse(init.body) : {}
      const path = String(url).split("/").at(-1)!
      calls.push({ path, body })
      if (path === "session" && init.method !== "DELETE" && !body.command) {
        sessionNumber++
        return Response.json({
          signedUrl: "wss://test",
          sessionId: `execution-${sessionNumber}`,
          conversationId: "transport-1",
          ...(body.history
            ? {
                history: {
                  conversationId: body.logicalConversationId ?? "logical-1",
                  sourceId: `source-${sessionNumber}`,
                  userId: "user-1",
                  context: '{"summary":"previous"}',
                  revision: 3,
                  coverage: "provisional",
                },
              }
            : {}),
        })
      }
      if (path === "attachments" && attachmentReply) return attachmentReply
      if (path === "history" && body.command === "delete")
        return Response.json(
          { status: failDelete ? "failed" : "deleted" },
          { status: failDelete ? 503 : 200 }
        )
      if (path === "history" && body.command === "events") {
        if (historyReply) return historyReply
        if (body.events[0]?.status === "queued" && queuedWrite)
          await queuedWrite
        return Response.json(
          { status: failEvents ? "failed" : "saved" },
          { status: failEvents ? 503 : 200 }
        )
      }
      if (path === "tools" && body.command === "execute" && issueReview)
        return Response.json({
          status: "requires_confirmation",
          data: { actionId: "action-1" },
          review: {
            ticket: "ticket-1",
            summary: "Create order",
            confirmationCode: "123456",
            confirmationPhrase: "Approve 123456",
            details: {},
            expiresAt: new Date(Date.now() + 600000).toISOString(),
          },
        })
      if (path === "history" && body.command === "get")
        return Response.json({
          conversation: {
            id: body.logicalConversationId,
            state: "active",
            revision: 3,
          },
          sources: [],
        })
      if (path === "history" && body.command === "messages")
        return Response.json({
          messages: [
            {
              id: "archived",
              source_id: "old",
              key: "old:user:1",
              ordinal: 1,
              role: "user",
              text: "Approve 123456",
              status: "queued",
              modality: "text",
              provenance: "client",
            },
          ],
          revision: 3,
          cursor: null,
          has_more: false,
          through_sequence: 1,
          gaps: [],
        })
      return Response.json({ status: "saved", data: [] })
    })
  )
})
afterEach(async () => {
  if (root) await act(async () => root.unmount())
  host?.remove()
  vi.unstubAllGlobals()
  sessionStorage.clear()
  vi.useRealTimers()
  Object.defineProperty(document, "visibilityState", {
    configurable: true,
    value: "visible",
  })
})

async function pendingUnauthorized(
  kind: "history" | "attachments",
  onUnauthorized?: () => void
) {
  let resolve!: (response: Response) => void
  const reply = new Promise<Response>((done) => {
    resolve = done
  })
  if (kind === "history") historyReply = reply
  else attachmentReply = reply
  await mount(true, onUnauthorized)
  const options = await start()
  let sent = Promise.resolve(true)
  await act(async () => {
    if (kind === "history")
      options.onMessage!({
        source: "user",
        role: "user",
        message: "pending history",
        event_id: 1,
      })
    else {
      vi.stubGlobal(
        "URL",
        class extends URL {
          static createObjectURL() {
            return "blob:attachment"
          }
          static revokeObjectURL() {}
        }
      )
      voice.choose([new File(["photo"], "photo.png", { type: "image/png" })])
      sent = voice.send("pending upload")
    }
    await Promise.resolve()
  })
  expect(calls.some((call) => call.path === kind)).toBe(true)
  return async () => {
    await act(async () => {
      resolve(Response.json({ message: "expired" }, { status: 401 }))
      await sent
      await new Promise((done) => setTimeout(done, 0))
    })
  }
}

describe("provider unauthorized identity ownership", () => {
  it.each([
    ["history", "token"],
    ["attachments", "token"],
    ["history", "workspaceId"],
    ["attachments", "workspaceId"],
  ] as const)(
    "ignores stale %s 401 after %s changes before React teardown",
    async (kind, field) => {
      const unauthorized = vi.fn()
      const finish = await pendingUnauthorized(kind, unauthorized)
      login[field] = "fresh-identity"
      await finish()
      expect(unauthorized).not.toHaveBeenCalled()
      expect(login[field]).toBe("fresh-identity")
    }
  )
  it.each(["history", "attachments"] as const)(
    "ignores unmounted %s 401 after fresh provider mounts",
    async (kind) => {
      const unauthorized = vi.fn()
      const finish = await pendingUnauthorized(kind, unauthorized)
      await act(async () => root.unmount())
      host.remove()
      login.token = "fresh-token"
      login.workspaceId = "fresh-workspace"
      await mount(true, unauthorized)
      await finish()
      expect(unauthorized).not.toHaveBeenCalled()
      expect(login.token).toBe("fresh-token")
    }
  )
  it.each(["history", "attachments"] as const)(
    "ignores unmounted %s 401 even if replacement keeps same identity",
    async (kind) => {
      const unauthorized = vi.fn()
      const finish = await pendingUnauthorized(kind, unauthorized)
      await act(async () => root.unmount())
      host.remove()
      await mount(true, unauthorized)
      await finish()
      expect(unauthorized).not.toHaveBeenCalled()
    }
  )
  it.each(["history", "attachments"] as const)(
    "expires current identity for live %s 401 through injected host callback",
    async (kind) => {
      const unauthorized = vi.fn()
      const finish = await pendingUnauthorized(kind, unauthorized)
      await finish()
      expect(unauthorized).toHaveBeenCalledOnce()
    }
  )
  it("uses default expiry handler for current session", async () => {
    const finish = await pendingUnauthorized("history")
    await finish()
    expect(login.signOut).toHaveBeenCalledWith("expired")
  })
})

describe("opt-in provider continuity", () => {
  it("leaves Office history disabled while supplying published prompt placeholder", async () => {
    await mount(false)
    await act(async () => voice.start(true))
    expect(calls.find((c) => c.path === "session")?.body).not.toHaveProperty(
      "history"
    )
    expect(sdk.sessions[0]?.dynamicVariables).toEqual({
      flow_history_context: "{}",
    })
    expect(calls.some((c) => c.path === "history")).toBe(false)
  })
  it("gates input until issued identity and initial context are installed", async () => {
    await mount()
    await act(async () => voice.start(true))
    expect(voice.connected).toBe(false)
    expect(await voice.send("early")).toBe(false)
    expect(sdk.sessions[0]?.dynamicVariables).toEqual({
      flow_history_context: '{"summary":"previous"}',
    })
    await act(async () =>
      sdk.sessions[0]!.onConnect!({ conversationId: "transport-1" })
    )
    expect(voice.connected).toBe(true)
    expect(voice.logicalConversationId).toBe("logical-1")
  })
  it("keeps draft on durable save failure and records sent uncertainty before clearing", async () => {
    await mount()
    await start()
    failEvents = true
    let sent = false
    await act(async () => {
      sent = await voice.send("first")
    })
    expect(sent).toBe(false)
    expect(sdk.sendUserMessage).not.toHaveBeenCalled()
    failEvents = false
    await act(async () => {
      sent = await voice.send("first")
    })
    expect(sent).toBe(true)
    expect(sdk.sendUserMessage).toHaveBeenCalledWith("first")
    expect(
      calls
        .filter((c) => c.path === "history" && c.body.command === "events")
        .at(-1)?.body.events[0]
    ).toMatchObject({
      text: "first",
      status: "sent_unconfirmed",
      modality: "text",
    })
  })
  it("hydrates archived approval text without starting SDK or replaying input", async () => {
    await mount()
    await act(async () => voice.restoreConversation("logical-older"))
    expect(voice.logicalConversationId).toBe("logical-older")
    expect(voice.messages[0]).toMatchObject({
      text: "Approve 123456",
      historical: true,
      delivery: "queued",
    })
    expect(sdk.startSession).not.toHaveBeenCalled()
    expect(sdk.sendUserMessage).not.toHaveBeenCalled()
  })
  it("continues same logical conversation with an empty greeting and ignores stale events", async () => {
    await mount()
    const old = await start()
    await act(async () =>
      old.onMessage!({
        source: "user",
        role: "user",
        message: "live",
        event_id: 1,
      })
    )
    await act(async () => voice.end())
    expect(voice.logicalConversationId).toBe("logical-1")
    expect(voice.messages.some((m) => m.text === "live")).toBe(true)
    await act(async () => voice.continueConversation("logical-1", true))
    expect(sdk.sessions.at(-1)?.overrides?.agent?.firstMessage).toBe("")
    await act(async () =>
      sdk.sessions.at(-1)!.onConnect!({ conversationId: "transport-1" })
    )
    await act(async () =>
      old.onMessage!({
        source: "user",
        role: "user",
        message: "stale",
        event_id: 2,
      })
    )
    expect(voice.messages.some((m) => m.text === "stale")).toBe(false)
    expect(sdk.sendUserMessage).not.toHaveBeenCalled()
  })
  it("keeps healthy capture resume on same SDK session and serializes microphone ownership", async () => {
    await mount()
    const held = new Set<string>()
    const request = vi.fn(
      async (
        key: string,
        _options: unknown,
        receive: (lock: object | null) => Promise<void>
      ) => {
        if (held.has(key)) return receive(null)
        held.add(key)
        try {
          await receive({})
        } finally {
          held.delete(key)
        }
      }
    )
    Object.defineProperty(navigator, "locks", {
      configurable: true,
      value: { request },
    })
    const options = await start(false)
    issueReview = true
    await act(async () => {
      await options.clientTools!.execute_genesy_tool!({
        operation_id: "orders_create",
        arguments_json: "{}",
      })
    })
    expect(voice.review?.ticket).toBe("ticket-1")
    await act(async () => voice.pauseCapture())
    expect(voice.capturePaused).toBe(true)
    await act(async () => voice.toggleMute())
    expect(voice.capturePaused).toBe(false)
    expect(voice.review?.ticket).toBe("ticket-1")
    expect(calls.some((c) => c.path === "confirm")).toBe(false)
    expect(sdk.startSession).toHaveBeenCalledTimes(1)
    expect(request).toHaveBeenCalledTimes(1)
    await act(async () => voice.end())
    expect(held.size).toBe(0)
  })
  it("does not request mic permission while another tab owns capture", async () => {
    await mount()
    await start()
    const getUserMedia = vi.fn(async () => ({ getTracks: () => [] }))
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: { getUserMedia },
    })
    Object.defineProperty(navigator, "locks", {
      configurable: true,
      value: {
        request: async (
          _name: string,
          _options: unknown,
          receive: (lock: null) => Promise<void>
        ) => receive(null),
      },
    })
    await act(async () => voice.upgrade())
    expect(getUserMedia).not.toHaveBeenCalled()
    expect(voice.connected).toBe(true)
    expect(sdk.startSession).toHaveBeenCalledTimes(1)
  })
  it("repairs lost capture on same SDK session before unmuting", async () => {
    const track = Object.assign(new EventTarget(), {
      readyState: "live",
      getSettings: () => ({ deviceId: "mic-1" }),
    })
    const media = Object.assign(new EventTarget(), {
      getUserMedia: async () => ({ getAudioTracks: () => [track] }),
    })
    Object.defineProperty(navigator, "mediaDevices", {
      configurable: true,
      value: media,
    })
    Object.defineProperty(navigator, "locks", {
      configurable: true,
      value: {
        request: async (
          _name: string,
          _options: unknown,
          receive: (lock: object) => Promise<void>
        ) => receive({}),
      },
    })
    await mount()
    await start(false)
    await media.getUserMedia()
    await act(async () => {
      track.readyState = "ended"
      track.dispatchEvent(new Event("ended"))
    })
    expect(voice.capturePaused).toBe(true)
    await act(async () => voice.toggleMute())
    expect(sdk.changeInputDevice).toHaveBeenCalledWith({})
    expect(voice.capturePaused).toBe(false)
    expect(sdk.startSession).toHaveBeenCalledTimes(1)
  })
  it("retains mic lease until SDK confirms capture teardown", async () => {
    await mount()
    let locked = false
    Object.defineProperty(navigator, "locks", {
      configurable: true,
      value: {
        request: async (
          _name: string,
          _options: unknown,
          receive: (lock: object | null) => Promise<void>
        ) => {
          locked = true
          try {
            await receive({})
          } finally {
            locked = false
          }
        },
      },
    })
    const options = await start(false)
    sdk.endSession.mockImplementation(() => {})
    let stopped!: Promise<void>
    await act(async () => {
      stopped = voice.end()
      await Promise.resolve()
    })
    expect(locked).toBe(true)
    await act(async () => {
      options.onStatusChange!({ status: "disconnected" })
      await stopped
    })
    expect(locked).toBe(false)
  })
  it("ends logical conversation as read-only and New creates a fresh identity", async () => {
    await mount()
    await start()
    await act(async () => voice.endConversation())
    expect(
      calls.some(
        (c) =>
          c.path === "history" &&
          c.body.command === "end" &&
          c.body.logicalConversationId === "logical-1"
      )
    ).toBe(true)
    await act(async () => voice.start(true))
    expect(sdk.startSession).toHaveBeenCalledTimes(1)
    await act(async () => voice.startNewConversation())
    expect(sdk.startSession).toHaveBeenCalledTimes(2)
    expect(
      calls.filter((c) => c.path === "session" && c.body.history).at(-1)?.body
    ).not.toHaveProperty("logicalConversationId")
  })
  it("stops active voice before deleting history and rejects old callbacks afterward", async () => {
    Object.defineProperty(navigator, "locks", {
      configurable: true,
      value: {
        request: async (
          _name: string,
          _options: unknown,
          receive: (lock: object) => Promise<void>
        ) => receive({}),
      },
    })
    await mount()
    const options = await start(false)
    await act(async () =>
      options.onMessage!({
        source: "user",
        role: "user",
        message: "keep until deletion",
        event_id: 1,
      })
    )
    expect(voice.messages).toHaveLength(1)
    sdk.endSession.mockImplementation(() => {})
    let deleting!: Promise<void>
    await act(async () => {
      deleting = voice.deleteConversation("logical-1")
      await Promise.resolve()
    })
    expect(
      calls.some((c) => c.path === "history" && c.body.command === "delete")
    ).toBe(false)
    expect(voice.messages).toHaveLength(1)
    await act(async () => {
      options.onStatusChange!({ status: "disconnected" })
      await deleting
    })
    expect(
      calls.filter((c) => c.path === "history" && c.body.command === "delete")
    ).toHaveLength(1)
    expect(voice.logicalConversationId).toBe(null)
    expect(voice.messages).toEqual([])
    expect(voice.connected).toBe(false)
    await act(async () => {
      options.onConnect!({ conversationId: "transport-1" })
      options.onMessage!({
        source: "user",
        role: "user",
        message: "stale after deletion",
        event_id: 2,
      })
    })
    expect(voice.messages).toEqual([])
    expect(voice.connected).toBe(false)
  })
  it("deletes another archived conversation while leaving current live session intact", async () => {
    await mount()
    await start()
    await act(async () => {
      await voice.send("live text")
    })
    const messages = voice.messages
    await act(async () => voice.deleteConversation("logical-other"))
    expect(voice.logicalConversationId).toBe("logical-1")
    expect(voice.connected).toBe(true)
    expect(voice.messages).toEqual(messages)
    expect(sdk.endSession).not.toHaveBeenCalled()
    expect(
      calls.find((c) => c.path === "history" && c.body.command === "delete")
        ?.body.logicalConversationId
    ).toBe("logical-other")
  })
  it("keeps deleted-call history and reports failure for retry after recording stops", async () => {
    await mount()
    const options = await start()
    await act(async () =>
      options.onMessage!({
        source: "user",
        role: "user",
        message: "keep on failure",
        event_id: 1,
      })
    )
    failDelete = true
    await act(async () => {
      await expect(voice.deleteConversation("logical-1")).rejects.toThrow()
    })
    expect(voice.connected).toBe(false)
    expect(voice.logicalConversationId).toBe("logical-1")
    expect(voice.messages.map((row) => row.text)).toEqual(["keep on failure"])
    expect(voice.error).not.toBe("")
    failDelete = false
    await act(async () => voice.deleteConversation("logical-1"))
    expect(voice.logicalConversationId).toBe(null)
    expect(voice.messages).toEqual([])
  })
  it("persists queued text then sends same row once after live reply", async () => {
    vi.useFakeTimers()
    await mount()
    const options = await start()
    await act(async () => {
      await voice.send("first")
    })
    await act(async () =>
      options.onMessage!({
        source: "ai",
        role: "agent",
        message: "checking",
        event_id: 5,
      })
    )
    await act(async () => {
      expect(await voice.send("second")).toBe(true)
    })
    const queued = calls
      .filter((c) => c.path === "history" && c.body.command === "events")
      .at(-1)!
    expect(queued.body.events[0]).toMatchObject({
      text: "second",
      status: "queued",
    })
    expect(sdk.sendUserMessage).toHaveBeenCalledTimes(1)
    await act(async () => vi.advanceTimersByTimeAsync(1500))
    expect(sdk.sendUserMessage).toHaveBeenNthCalledWith(2, "second")
    const sent = calls
      .filter((c) => c.path === "history" && c.body.command === "events")
      .at(-1)!
    expect(sent.body.events[0]).toMatchObject({
      key: queued.body.events[0]!.key,
      ordinal: queued.body.events[0]!.ordinal,
      status: "sent_unconfirmed",
    })
    expect(voice.messages.filter((m) => m.text === "second")).toHaveLength(1)
    expect(voice.messages.find((m) => m.text === "second")?.delivery).toBe(
      "sent_unconfirmed"
    )
  })
  it("never moves a pending old-source text write into a resumed source", async () => {
    await mount()
    const options = await start()
    await act(async () =>
      options.onMessage!({
        source: "user",
        role: "user",
        message: "start",
        event_id: 1,
      })
    )
    await act(async () =>
      options.onMessage!({
        source: "ai",
        role: "agent",
        message: "checking",
        event_id: 2,
      })
    )
    let release!: () => void
    queuedWrite = new Promise<void>((resolve) => {
      release = resolve
    })
    let sent!: Promise<boolean>
    await act(async () => {
      sent = voice.send("old pending")
      await Promise.resolve()
    })
    await act(async () => options.onDisconnect!({ reason: "agent" }))
    await act(async () => voice.resume())
    await act(async () =>
      sdk.sessions.at(-1)!.onConnect!({ conversationId: "transport-1" })
    )
    await act(async () => {
      release()
      await sent
    })
    expect(await sent).toBe(false)
    expect(voice.messages.some((m) => m.text === "old pending")).toBe(false)
    expect(sdk.sendUserMessage).not.toHaveBeenCalled()
  })
  it.each([true, false])(
    "keeps hidden-page review cancellation in both hosts (history=%s)",
    async (historyEnabled) => {
      await mount(historyEnabled)
      const options = await start()
      issueReview = true
      await act(async () => {
        await options.clientTools!.execute_genesy_tool!({
          operation_id: "orders_create",
          arguments_json: "{}",
        })
      })
      expect(voice.review?.ticket).toBe("ticket-1")
      await act(async () => {
        Object.defineProperty(document, "visibilityState", {
          configurable: true,
          value: "hidden",
        })
        document.dispatchEvent(new Event("visibilitychange"))
      })
      expect(voice.review).toBe(null)
      expect(
        calls.some((c) => c.path === "confirm" && c.body.command === undefined)
      ).toBe(true)
    }
  )
  it("deduplicates response resends and corrects first event under stable source response key", async () => {
    await mount()
    const options = await start()
    await act(async () =>
      options.onMessage!({
        source: "ai",
        role: "agent",
        message: "before",
        event_id: 4,
        response_id: "response-4",
      })
    )
    await act(async () =>
      options.onMessage!({
        source: "ai",
        role: "agent",
        message: "before, updated",
        event_id: 5,
        response_id: "response-4",
      })
    )
    await act(async () =>
      options.onAgentResponseCorrection!({
        event_id: 4,
        original_agent_response: "before",
        corrected_agent_response: "after",
      })
    )
    const events = calls
      .filter((c) => c.path === "history" && c.body.command === "events")
      .map((c) => c.body.events[0])
    expect(events).toHaveLength(3)
    expect(new Set(events.map((event) => event?.key))).toEqual(
      new Set(["source-1:agent:response-4"])
    )
    expect(new Set(events.map((event) => event?.ordinal)).size).toBe(1)
    expect(events.at(-1)).toMatchObject({
      key: "source-1:agent:response-4",
      text: "after",
    })
    expect(events.at(-1)?.ordinal).toBe(events.at(-2)?.ordinal)
    await act(async () => options.onInterruption!({ event_id: 4 }))
    expect(
      calls
        .filter((c) => c.path === "history" && c.body.command === "events")
        .at(-1)?.body.events[0]
    ).toMatchObject({
      key: "source-1:agent:response-4",
      interrupted: true,
      text: "after",
    })
    expect(
      voice.messages.filter((m) => m.role === "agent").map((m) => m.text)
    ).toEqual(["after"])
  })
})

describe("SDK startup error context", () => {
  it("maps microphone denial from the SDK callback without exposing its raw message", async () => {
    await mount()
    await act(async () => voice.start(true))
    const options = sdk.sessions.at(-1)!
    await act(async () =>
      options.onError?.(
        "raw browser permission failure",
        new DOMException("Permission denied", "NotAllowedError")
      )
    )
    expect(voice.error).toBe("microphoneDenied")
    expect(voice.error).not.toContain("Permission denied")
  })
})
