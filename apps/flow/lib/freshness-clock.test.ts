import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { createFreshnessClock } from "./freshness-clock"

function setup() {
  let hidden = false
  let show: () => void = () => undefined
  const clock = createFreshnessClock({
    isHidden: () => hidden,
    onVisible: (cb) => {
      show = cb
      return () => (show = () => undefined)
    },
  })
  return {
    clock,
    hide: () => (hidden = true),
    reveal: () => {
      hidden = false
      show()
    },
  }
}

describe("shared freshness clock", () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it("runs one timer for many subscribers, notifying each every 30 s while young", () => {
    const { clock } = setup()
    const a = vi.fn()
    const b = vi.fn()
    clock.subscribe(a, () => 0)
    clock.subscribe(b, () => 0)
    expect(vi.getTimerCount()).toBe(1)
    vi.advanceTimersByTime(29_999)
    expect(a).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(a).toHaveBeenCalledTimes(1)
    expect(b).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(1)
  })

  it("slows to 60 s once every subscriber's data is 10 minutes old", () => {
    const { clock } = setup()
    const a = vi.fn()
    clock.subscribe(a, () => 700_000)
    vi.advanceTimersByTime(59_999)
    expect(a).not.toHaveBeenCalled()
    vi.advanceTimersByTime(1)
    expect(a).toHaveBeenCalledTimes(1)
  })

  it("clears its timer when the last subscriber leaves", () => {
    const { clock } = setup()
    const off1 = clock.subscribe(vi.fn(), () => 0)
    const off2 = clock.subscribe(vi.fn(), () => 0)
    off1()
    expect(vi.getTimerCount()).toBe(1)
    off2()
    expect(vi.getTimerCount()).toBe(0)
  })

  it("pauses while hidden and re-arms with an immediate notify on return", () => {
    const { clock, hide, reveal } = setup()
    const a = vi.fn()
    clock.subscribe(a, () => 0)
    hide()
    vi.advanceTimersByTime(30_000)
    expect(a).not.toHaveBeenCalled()
    expect(vi.getTimerCount()).toBe(0)
    reveal()
    expect(a).toHaveBeenCalledTimes(1)
    expect(vi.getTimerCount()).toBe(1)
  })
})
