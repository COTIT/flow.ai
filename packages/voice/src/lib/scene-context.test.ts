import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { contextSync, contextUpdate, sceneContext } from "./scene-context"

describe("scene context", () => {
  it("names the scene from the path and carries ids only", () => {
    expect(sceneContext({ pathname: "/" })).toEqual({ active_scene: "home" })
    expect(sceneContext({ pathname: "/storefront" })).toEqual({
      active_scene: "storefront",
    })
    expect(
      sceneContext({
        pathname: "/scenes/stock",
        selected: { type: "product", ids: ["p1"] },
      })
    ).toEqual({
      active_scene: "stock",
      selected_record_type: "product",
      selected_record_id: "p1",
    })
    expect(
      sceneContext({
        pathname: "/scenes/orders",
        selected: { type: "order", ids: ["o1", "o2"] },
      })
    ).toEqual({
      active_scene: "orders",
      selected_record_type: "order",
      selected_record_ids: ["o1", "o2"],
    })
  })

  it("wraps the context as untrusted UI state", () => {
    const sent = JSON.parse(contextUpdate({ active_scene: "home" }))
    expect(sent.flow_context).toEqual({ active_scene: "home" })
    expect(sent.instruction).toMatch(/UNTRUSTED/)
    expect(sent.instruction).toMatch(/never answer it aloud/i)
  })
})

describe("context sync", () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  const fake = (connected = true) => {
    const conversation = {
      sendContextualUpdate: vi.fn(),
      sendUserMessage: vi.fn(),
    }
    const sync = contextSync(
      () => (connected ? conversation : null),
      300
    )
    return { conversation, sync }
  }

  it("sends one quiet update per scene change, after the debounce, never a user message", () => {
    const { conversation, sync } = fake()
    sync.update({ active_scene: "home" })
    sync.update({ active_scene: "storefront" })
    expect(conversation.sendContextualUpdate).not.toHaveBeenCalled()
    vi.advanceTimersByTime(300)
    expect(conversation.sendContextualUpdate).toHaveBeenCalledTimes(1)
    expect(
      JSON.parse(conversation.sendContextualUpdate.mock.calls[0]![0])
        .flow_context
    ).toEqual({ active_scene: "storefront" })
    expect(conversation.sendUserMessage).not.toHaveBeenCalled()
  })

  it("skips a context equal to the last one sent", () => {
    const { conversation, sync } = fake()
    sync.update({ active_scene: "home" })
    vi.advanceTimersByTime(300)
    sync.update({ active_scene: "home" })
    vi.advanceTimersByTime(300)
    expect(conversation.sendContextualUpdate).toHaveBeenCalledTimes(1)
  })

  it("sends nothing while disconnected, and resends after a reset (new session)", () => {
    const off = fake(false)
    off.sync.update({ active_scene: "home" })
    vi.advanceTimersByTime(300)
    expect(off.conversation.sendContextualUpdate).not.toHaveBeenCalled()

    const { conversation, sync } = fake()
    sync.update({ active_scene: "home" })
    vi.advanceTimersByTime(300)
    sync.reset()
    sync.update({ active_scene: "home" })
    vi.advanceTimersByTime(300)
    expect(conversation.sendContextualUpdate).toHaveBeenCalledTimes(2)
  })

  it("drops a pending update on cancel", () => {
    const { conversation, sync } = fake()
    sync.update({ active_scene: "home" })
    sync.cancel()
    vi.advanceTimersByTime(300)
    expect(conversation.sendContextualUpdate).not.toHaveBeenCalled()
  })
})
