import { beforeEach, describe, expect, it } from "vitest"

import { flowStore } from "./flow-store"

beforeEach(() => flowStore.setState(flowStore.getInitialState()))

describe("flow store", () => {
  it("keeps the unsent text until it is cleared", () => {
    flowStore.getState().setDraft("half typed")
    expect(flowStore.getState().draft).toBe("half typed")
    flowStore.getState().setDraft("")
    expect(flowStore.getState().draft).toBe("")
  })

  it("forgets an explicit end once a new session starts", () => {
    flowStore.getState().ended()
    expect(flowStore.getState().endedByUser).toBe(true)
    expect(flowStore.getState().immersive).toBe(false)
    flowStore.getState().started()
    expect(flowStore.getState().endedByUser).toBe(false)
    expect(flowStore.getState().immersive).toBe(true)
  })
})
