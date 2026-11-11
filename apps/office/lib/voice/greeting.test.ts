import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { greetingText, sessionGreeting } from "./greeting"

const context = {
  token: "staff",
  workspaceId: "w1",
  apiUrl: "https://api.test/v1",
}
const priorities = (groups: unknown[]) =>
  new Response(JSON.stringify({ data: { restricted: [], groups } }), {
    headers: { "content-type": "application/json" },
  })
const group = (snoozed = false) => ({
  kind: "stock.below_minimum",
  count: 2,
  snoozed,
  items: [{ label: "Honey", money: { amount: 999999 } }],
})

beforeEach(() => vi.stubEnv("ASSISTANT_GREETING_OVERRIDE", "on"))
afterEach(() => {
  vi.unstubAllEnvs()
  vi.unstubAllGlobals()
})

it("a command opens with no greeting, so the agent answers the command", () => {
  for (const language of ["en", "nl", "sw"])
    expect(greetingText({ language, hint: "command" })).toBe("")
})

it("names only how many things need attention, never their values, in each language", () => {
  expect(greetingText({ language: "en", attention: 2 })).toBe(
    "Hi, welcome back. 2 things need your attention. Where shall we start?"
  )
  expect(greetingText({ language: "en", attention: 1 })).toContain(
    "One thing needs your attention."
  )
  expect(greetingText({ language: "nl", attention: 3 })).toContain(
    "Er vragen 3 dingen om je aandacht."
  )
  expect(greetingText({ language: "sw", attention: 1 })).toContain(
    "Kuna jambo moja linalohitaji umakini wako."
  )
})

it("offers to continue a recoverable task before anything else", () => {
  expect(greetingText({ language: "en", hint: "resume", attention: 4 })).toBe(
    "Welcome back. Your earlier task is still here. Shall we continue?"
  )
})

it("nothing to say keeps the agent's own greeting", () => {
  expect(greetingText({ language: "en" })).toBeUndefined()
  expect(greetingText({ language: "en", attention: 0 })).toBeUndefined()
})

it("is off unless ASSISTANT_GREETING_OVERRIDE is on, and then reads nothing", async () => {
  const fetch = vi.fn()
  vi.stubGlobal("fetch", fetch)
  vi.stubEnv("ASSISTANT_GREETING_OVERRIDE", "")
  expect(await sessionGreeting(context, "en", "command")).toBeUndefined()
  expect(await sessionGreeting(context, "en", undefined)).toBeUndefined()
  expect(fetch).not.toHaveBeenCalled()
})

it("counts the caller's not-snoozed priority groups with the caller's own bearer", async () => {
  const fetch = vi.fn(async () => priorities([group(), group(), group(true)]))
  vi.stubGlobal("fetch", fetch)
  expect(await sessionGreeting(context, "en", undefined)).toBe(
    "Hi, welcome back. 2 things need your attention. Where shall we start?"
  )
  expect(fetch).toHaveBeenCalledWith(
    "https://api.test/v1/workspace/w1/attention/priorities",
    expect.objectContaining({
      headers: expect.objectContaining({ Authorization: "Bearer staff" }),
    })
  )
})

it("a command or resume needs no read; a failed read never blocks the session", async () => {
  const fetch = vi.fn(async () => new Response("no", { status: 500 }))
  vi.stubGlobal("fetch", fetch)
  expect(await sessionGreeting(context, "en", "command")).toBe("")
  expect(await sessionGreeting(context, "nl", "resume")).toContain(
    "Welkom terug."
  )
  expect(fetch).not.toHaveBeenCalled()
  expect(await sessionGreeting(context, "en", undefined)).toBeUndefined()
  fetch.mockRejectedValueOnce(new Error("down"))
  expect(await sessionGreeting(context, "en", undefined)).toBeUndefined()
})
