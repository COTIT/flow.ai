import { afterEach, beforeEach, expect, it, vi } from "vitest"

const spans = vi.hoisted(() => [] as string[])
vi.mock("./tracing", () => ({
  withVoiceTracing: async (name: string, action: () => Promise<unknown>) => {
    spans.push(name)
    return action()
  },
}))
import { authorizeVoiceSession } from "./identity"

const ws = "01950000-0000-7000-8000-000000000009"
const sessionId = "01950000-0000-7000-8000-000000000001"
const live = {
  id: sessionId,
  state: "active",
  language: "nl",
  conversation_id: "conv",
  expires_at: new Date(Date.now() + 60000).toISOString(),
}
const request = (auth = "Bearer staff") =>
  new Request("http://office.test/api/voice/tools", {
    headers: { Authorization: auth },
  })
function stub(routes: Record<string, () => Response>) {
  const fetcher = vi.fn(async (url: string | URL) => {
    const href = String(url)
    const key = Object.keys(routes).find((suffix) => href.endsWith(suffix))
    return key ? routes[key]!() : new Response(null, { status: 404 })
  })
  vi.stubGlobal("fetch", fetcher)
  return fetcher
}
const identity = (extra: Record<string, unknown> = {}) =>
  Response.json({
    data: {
      ...live,
      identity: {
        workspaces: [ws, "other"],
        permissions: ["order.get"],
        farm_roles: ["ops", "field_officer", "superuser"],
        ...extra,
      },
    },
  })

beforeEach(() => {
  spans.length = 0
  vi.stubEnv("API_URL", "https://api.test/v1")
})
afterEach(() => {
  vi.unstubAllGlobals()
  vi.unstubAllEnvs()
})

it("verifies login, workspace, permissions, farm roles and session in one backend call", async () => {
  const fetcher = stub({ [`/sessions/${sessionId}?with=identity`]: identity })
  const { context, session } = await authorizeVoiceSession(
    request(),
    ws,
    sessionId,
    "live"
  )
  expect(fetcher).toHaveBeenCalledOnce()
  expect(String(fetcher.mock.calls[0]![0])).toBe(
    `https://api.test/v1/workspace/${ws}/assistant/sessions/${sessionId}?with=identity`
  )
  expect(context).toMatchObject({
    token: "staff",
    workspaceId: ws,
    apiUrl: "https://api.test/v1",
    sessionId,
    language: "nl",
  })
  expect(context.permissions).toEqual([
    "order.get",
    "farm:ops",
    "farm:field_officer",
  ])
  expect(session.id).toBe(sessionId)
  expect(spans).toEqual(["session_lookup"])
})

it("falls back to the separate login checks when the backend has no identity read", async () => {
  const fetcher = stub({
    [`/sessions/${sessionId}?with=identity`]: () =>
      Response.json({ data: live }),
    "/me/workspaces": () => Response.json({ data: [{ id: ws }] }),
    "/farm/me": () =>
      Response.json({
        data: { memberships: [{ workspace_id: ws, role: "farmer" }] },
      }),
    "/me": () =>
      Response.json({
        data: {
          user: {
            permissions: [{ workspace: ws, permissions: ["order.get"] }],
          },
        },
      }),
  })
  const { context } = await authorizeVoiceSession(
    request(),
    ws,
    sessionId,
    "live"
  )
  expect(fetcher).toHaveBeenCalledTimes(4)
  expect(context.permissions).toEqual(["order.get", "farm:farmer"])
  expect(spans).toEqual(["session_lookup", "authorize"])
})

it("refuses a workspace missing from the bearer's own memberships", async () => {
  stub({
    [`/sessions/${sessionId}?with=identity`]: () =>
      identity({ workspaces: ["other"] }),
  })
  await expect(
    authorizeVoiceSession(request(), ws, sessionId, "live")
  ).rejects.toMatchObject({
    status: 403,
    message: "You do not have access to this workspace.",
  })
})

it.each([
  [401, "Your session could not be verified."],
  [403, "You do not have access to this workspace."],
])("maps backend %s to the existing login message", async (status, message) => {
  stub({
    [`/sessions/${sessionId}?with=identity`]: () =>
      Response.json({ message: "raw" }, { status }),
  })
  await expect(
    authorizeVoiceSession(request(), ws, sessionId, "read")
  ).rejects.toMatchObject({ status, message })
})

it("requires a live session for new work but lets status reads see ended sessions", async () => {
  stub({
    [`/sessions/${sessionId}?with=identity`]: () =>
      Response.json({
        data: {
          ...live,
          state: "ended",
          identity: { workspaces: [ws], permissions: [], farm_roles: [] },
        },
      }),
  })
  await expect(
    authorizeVoiceSession(request(), ws, sessionId, "live")
  ).rejects.toMatchObject({ status: 410 })
  await expect(
    authorizeVoiceSession(request(), ws, sessionId, "read")
  ).resolves.toMatchObject({ session: { state: "ended" } })
})

it("rejects malformed login, workspace and session input before any backend call", async () => {
  const fetcher = stub({})
  await expect(
    authorizeVoiceSession(request("Basic x"), ws, sessionId, "live")
  ).rejects.toMatchObject({ status: 401 })
  await expect(
    authorizeVoiceSession(request(), "../x", sessionId, "live")
  ).rejects.toMatchObject({ status: 400 })
  await expect(
    authorizeVoiceSession(request(), ws, "../x", "live")
  ).rejects.toMatchObject({ status: 400 })
  expect(fetcher).not.toHaveBeenCalled()
})
