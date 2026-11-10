import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest"
import { readdirSync } from "node:fs"
import { join } from "node:path"

vi.mock("server-only", () => ({}))
vi.mock("./settings", () => ({
  voiceSetting: vi.fn(async (name: string) =>
    name === "ASSISTANT_BRIDGE_SECRET" ? "s".repeat(40) : ""
  ),
}))

const context = {
  token: "staff-token",
  workspaceId: "11111111-1111-4111-8111-111111111111",
  permissions: [],
  apiUrl: "https://api.test/v1",
  sessionId: "22222222-2222-4222-8222-222222222222",
  language: "sw",
}
const gapId = "33333333-3333-4333-8333-333333333333"

// First import builds the catalog vocabulary (slow on a cold worker).
beforeAll(() => import("./gaps"), 30_000)
beforeEach(() =>
  Reflect.deleteProperty(globalThis, Symbol.for("genesy.office.voice.gaps.v1"))
)
afterEach(() => {
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

function api(status = 201) {
  const fetcher = vi.fn<
    (url: string | URL, init?: RequestInit) => Promise<Response>
  >(
    async () =>
      new Response(JSON.stringify({ data: { id: gapId } }), { status })
  )
  vi.stubGlobal("fetch", fetcher)
  return fetcher
}
const sent = (fetcher: ReturnType<typeof api>, call = 0) => ({
  url: String(fetcher.mock.calls[call]![0]),
  body: JSON.parse(String(fetcher.mock.calls[call]![1]!.body)),
  headers: fetcher.mock.calls[call]![1]!.headers as Record<string, string>,
})

it("keeps only catalog words, so names, numbers and emails never leave Office", async () => {
  const { redactIntent } = await import("./gaps")
  expect(
    redactIntent("Create credit note for order 1234 for John (john@x.tz)")
  ).toBe("create credit note for order for")
  expect(redactIntent("Tengeneza oda mpya kwa Mary")).toBe("unknown")
  expect(redactIntent("order")).toBe("unknown")
  expect(redactIntent("")).toBe("unknown")
  expect(redactIntent("order ".repeat(40)).split(" ").length).toBe(12)
})

it("records a refused tool call with its domain, language, tools and status", async () => {
  const fetcher = api()
  const { noteToolCall, recordGap } = await import("./gaps")
  noteToolCall(context, "orders_get")
  expect(
    await recordGap(context, {
      outcome: "refused",
      operationId: "orders_create",
      httpStatus: 422,
    })
  ).toBe(gapId)
  const call = sent(fetcher)
  expect(call.url).toBe(
    `https://api.test/v1/workspace/${context.workspaceId}/assistant/gaps`
  )
  expect(call.headers.Authorization).toBe("Bearer staff-token")
  expect(call.headers["Assistant-Bridge-Signature"]).toMatch(/^[0-9a-f]{64}$/)
  expect(call.body).toEqual({
    domain: "orders",
    language: "sw",
    intent: expect.stringMatching(/^[a-z ]+$/),
    tools: ["orders_get", "orders_create"],
    outcome: "refused",
    http_status: 422,
  })
})

it("prefers the agent's English search phrase and still counts non-English gaps", async () => {
  const fetcher = api()
  const { recordGap } = await import("./gaps")
  await recordGap(context, {
    outcome: "unsupported",
    query: "create credit note for order 77 for Ruaha",
  })
  expect(sent(fetcher).body).toMatchObject({
    intent: "create credit note for order for",
    domain: "orders",
    tools: [],
  })
  await recordGap(context, { outcome: "dropped", text: "Nataka oda" })
  expect(sent(fetcher, 1).body).toMatchObject({
    intent: "unknown",
    domain: "unknown",
    outcome: "dropped",
    language: "sw",
  })
})

it("never throws or waits long when the ledger is down", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise<Response>(() => {}))
  )
  const { recordGap } = await import("./gaps")
  const started = Date.now()
  expect(await recordGap(context, { outcome: "failed" })).toBeUndefined()
  expect(Date.now() - started).toBeLessThan(1500)
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => {
      throw new Error("down")
    })
  )
  expect(await recordGap(context, { outcome: "failed" })).toBeUndefined()
})

it("marks a gap rephrased when the same session calls a tool within 60 s, once", async () => {
  const fetcher = api()
  const { noteToolCall, recordGap } = await import("./gaps")
  await recordGap(context, { outcome: "unsupported", query: "credit note" })
  noteToolCall(context, "orders_get")
  noteToolCall(context, "orders_list")
  await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(2))
  const follow = sent(fetcher, 1)
  expect(follow.url).toBe(
    `https://api.test/v1/assistant/gaps/${gapId}/follow-up`
  )
  expect(follow.body).toEqual({
    workspace_id: context.workspaceId,
    signal: "rephrased",
  })
  noteToolCall({ ...context, sessionId: "other" }, "orders_get")
  expect(fetcher).toHaveBeenCalledTimes(2)
})

it("does not call a late tool call a rephrase", async () => {
  vi.useFakeTimers({ toFake: ["Date"] })
  const fetcher = api()
  const { noteToolCall, recordGap } = await import("./gaps")
  await recordGap(context, {
    outcome: "cancelled",
    operationId: "orders_create",
  })
  vi.setSystemTime(Date.now() + 61_000)
  noteToolCall(context, "orders_get")
  expect(fetcher).toHaveBeenCalledTimes(1)
})

it("signs the follow-up cookie and turns record paths into route patterns", async () => {
  const { gapCookie, readGapCookie, routePattern } =
    await import("./gap-follow-up")
  const value = await gapCookie(context.workspaceId, gapId)
  expect(await readGapCookie(value)).toEqual({
    workspaceId: context.workspaceId,
    gapId,
  })
  expect(await readGapCookie(value.replace(/.$/, "0"))).toBeUndefined()
  expect(await readGapCookie("junk")).toBeUndefined()
  expect(routePattern(`/orders/${gapId}`)).toBe("/orders/[id]")
  expect(routePattern("/settings/app-keys")).toBe("/settings/app-keys")
  expect(routePattern("/settings/storefront")).toBe("/settings/storefront")
  expect(routePattern("/invoices/1042")).toBe("/invoices/[id]")
  expect(routePattern("/products/afm-coffee-250g")).toBe("/products/[id]")
  expect(routePattern("/voice")).toBeUndefined()
  expect(routePattern("/voice/secure")).toBeUndefined()
  expect(routePattern("/api/voice/tools")).toBeUndefined()
  expect(routePattern("/")).toBeUndefined()
  // Browser files (robots, icons, service worker) are not a page the user chose.
  expect(routePattern("/robots.txt")).toBeUndefined()
  expect(routePattern("/icons/apple-touch-icon.png")).toBeUndefined()
  // Too long to send whole: dropped, never cut mid-segment.
  expect(routePattern(`/orders${"/x".repeat(40)}`)).toBeUndefined()
})

it("knows every static Office page segment, so new pages keep their names", async () => {
  const { ROUTE_SEGMENTS } = await import("./gap-follow-up")
  const found = new Set<string>()
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      if (!entry.isDirectory() || (dir === "app" && entry.name === "api"))
        continue
      if (!/^[[(]/.test(entry.name)) found.add(entry.name)
      walk(join(dir, entry.name))
    }
  }
  walk("app")
  expect([...found].sort()).toEqual([...ROUTE_SEGMENTS].sort())
})

it("tracks a failed tool response as a gap and sets the signed follow-up cookie", async () => {
  const fetcher = api()
  const { trackGap } = await import("./gaps")
  const { readGapCookie } = await import("./gap-follow-up")
  const refused = Response.json(
    { status: "failed", reason: "invalid", httpStatus: 422 },
    { status: 200 }
  )
  const out = await trackGap({ context, operationId: "orders_get" }, refused)
  expect(await out.json()).toMatchObject({ reason: "invalid" })
  expect(sent(fetcher).body).toMatchObject({
    outcome: "refused",
    http_status: 422,
    tools: ["orders_get"],
  })
  const cookie = out.headers.get("set-cookie") ?? ""
  expect(cookie).toMatch(
    /^genesy_voice_gap=[^;]+; Path=\/; Max-Age=300; HttpOnly; Secure; SameSite=Lax$/
  )
  expect(
    await readGapCookie(/genesy_voice_gap=([^;]+)/.exec(cookie)?.[1])
  ).toEqual({ workspaceId: context.workspaceId, gapId })
})

it("maps unsupported, platform faults and cancels; ignores successes", async () => {
  const fetcher = api()
  const { trackGap } = await import("./gaps")
  await trackGap(
    { context, query: "unsupported: credit note for order 9" },
    Response.json({ tools: [], reason: "unsupported" })
  )
  await trackGap(
    { context },
    Response.json({ reason: "unverified" }, { status: 503 })
  )
  await trackGap(
    { context, operationId: "orders_create", cancelled: true },
    Response.json({ status: "cancelled" })
  )
  const ok = await trackGap({ context }, Response.json({ status: "completed" }))
  expect(ok.headers.get("set-cookie")).toBeNull()
  expect(
    await trackGap({}, Response.json({ reason: "invalid" }, { status: 400 }))
  ).toBeInstanceOf(Response)
  expect(
    fetcher.mock.calls.map((_, i) => sent(fetcher, i).body.outcome)
  ).toEqual(["unsupported", "failed", "cancelled"])
  expect(sent(fetcher).body.intent).toBe("credit note for order")
  expect(sent(fetcher, 1).body.http_status).toBe(503)
})
