import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { NextRequest } from "next/server"

vi.mock("server-only", () => ({}))
vi.mock("./settings", () => ({
  voiceSetting: vi.fn(async (name: string) =>
    name === "ASSISTANT_BRIDGE_SECRET"
      ? "s".repeat(40)
      : name === "API_URL"
        ? "https://api.test/v1"
        : ""
  ),
}))

const workspace = "11111111-1111-4111-8111-111111111111"
const gap = "33333333-3333-4333-8333-333333333333"
let fetcher: ReturnType<typeof vi.fn>
beforeEach(() => {
  fetcher = vi.fn(async () => new Response("{}"))
  vi.stubGlobal("fetch", fetcher)
})
afterEach(() => vi.unstubAllGlobals())

async function visit(path: string, cookie?: string, method = "GET") {
  const { proxy } = await import("@/proxy")
  const request = new NextRequest(`https://office.test${path}`, {
    method,
    headers: cookie ? { cookie: `genesy_voice_gap=${cookie}` } : {},
  })
  return proxy(request)
}

it(
  "records the next Office page after a voice gap as a route pattern, once",
  { timeout: 30_000 },
  async () => {
    const { gapCookie } = await import("./gap-follow-up")
    const response = await visit(
      `/orders/${gap}?search=Mary`,
      await gapCookie(workspace, gap)
    )
    expect(response.headers.get("Content-Security-Policy")).toBeTruthy()
    expect(response.headers.get("set-cookie")).toMatch(/genesy_voice_gap=;/)
    // First follow-up lazily loads the bridge signer (and its catalog import).
    await vi.waitFor(() => expect(fetcher).toHaveBeenCalledTimes(1), {
      timeout: 15_000,
    })
    const [url, init] = fetcher.mock.calls[0]!
    expect(String(url)).toBe(
      `https://api.test/v1/assistant/gaps/${gap}/follow-up`
    )
    expect(JSON.parse(String(init.body))).toEqual({
      workspace_id: workspace,
      signal: "used_ui",
      route: "/orders/[id]",
    })
    expect(init.headers["Assistant-Bridge-Signature"]).toMatch(/^[0-9a-f]{64}$/)
  }
)

it("never waits for the ledger or fails the page", async () => {
  fetcher.mockImplementation(() => new Promise(() => {}))
  const { gapCookie } = await import("./gap-follow-up")
  const started = Date.now()
  const response = await visit("/invoices", await gapCookie(workspace, gap))
  expect(Date.now() - started).toBeLessThan(200)
  expect(response.status).toBe(200)
})

it("ignores voice pages, missing and forged cookies", async () => {
  const { gapCookie } = await import("./gap-follow-up")
  const kept = await visit("/voice", await gapCookie(workspace, gap))
  expect(kept.headers.get("set-cookie")).toBeNull()
  expect((await visit("/orders")).headers.get("set-cookie")).toBeNull()
  const posted = await visit("/orders", await gapCookie(workspace, gap), "POST")
  expect(posted.headers.get("set-cookie")).toBeNull()
  const forged = await visit("/orders", `${workspace}.${gap}.${"0".repeat(64)}`)
  expect(forged.headers.get("set-cookie")).toMatch(/genesy_voice_gap=;/)
  await new Promise((resolve) => setTimeout(resolve, 20))
  expect(fetcher).not.toHaveBeenCalled()
})
