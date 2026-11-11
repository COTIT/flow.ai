import { expect, it } from "vitest"
import { agentOverride, greetingHint } from "./greeting"

it("asks for no greeting when the user opened with text, and offers a resume otherwise", () => {
  expect(greetingHint("command", 2)).toBe("command")
  expect(greetingHint(undefined, 1)).toBe("resume")
  expect(greetingHint(undefined, 0)).toBeUndefined()
})

it("overrides the first message only when Office sent one (the override is off by default)", () => {
  expect(agentOverride("nl", undefined)).toEqual({ language: "nl" })
  expect(agentOverride("nl", 42)).toEqual({ language: "nl" })
  expect(agentOverride("en", "")).toEqual({ language: "en", firstMessage: "" })
  expect(agentOverride("sw", "Karibu tena.")).toEqual({ language: "sw", firstMessage: "Karibu tena." })
})
