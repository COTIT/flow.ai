import { afterEach, expect, it, vi } from "vitest"
import { workspaceTimeZone } from "./time-zone"

afterEach(() => vi.unstubAllGlobals())

const context = {
  token: "bearer",
  workspaceId: "w1",
  permissions: [],
  apiUrl: "https://api.test/v1",
}

it("reads the workspace's own time zone setting with the caller's token", async () => {
  const fetch = vi.fn(async () =>
    Response.json({ data: "Africa/Dar_es_Salaam" })
  )
  vi.stubGlobal("fetch", fetch)
  expect(await workspaceTimeZone(context)).toBe("Africa/Dar_es_Salaam")
  const [url, init] = fetch.mock.calls[0] as unknown as [string, RequestInit]
  expect(url).toBe(
    "https://api.test/v1/workspace/w1/settings?type=general.timezone"
  )
  expect(new Headers(init.headers).get("Authorization")).toBe("Bearer bearer")
})

it.each([
  ["no setting.get permission", () => Response.json({}, { status: 403 })],
  ["not a real zone", () => Response.json({ data: "Mars/Olympus" })],
  ["not a string", () => Response.json({ data: { zone: "UTC" } })],
  [
    "backend down",
    () => {
      throw new TypeError("fetch failed")
    },
  ],
])("leaves the zone out when %s", async (_, respond) => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => respond())
  )
  expect(await workspaceTimeZone(context)).toBeUndefined()
})
