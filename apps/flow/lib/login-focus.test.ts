import { expect, it } from "vitest"

import { focusLoginOnArrival, takeLoginFocus } from "./login-focus"

it("focus is taken once per arrival Flow announced, never on its own", () => {
  expect(takeLoginFocus()).toBe(false)
  focusLoginOnArrival()
  expect(takeLoginFocus()).toBe(true)
  expect(takeLoginFocus()).toBe(false)
})
