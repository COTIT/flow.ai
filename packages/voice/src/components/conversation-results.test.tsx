import { afterEach, describe, expect, it, vi } from "vitest"
import { captureScroll } from "./conversation-results"

class Pane {
  scrollHeight = 100
  clientHeight = 100
  scrollWidth = 100
  clientWidth = 100
  scrollTop = 0
  scrollLeft = 0
  isConnected = true
  parentElement: Pane | null = null
}

afterEach(() => vi.unstubAllGlobals())

describe("conversation modal scroll snapshot", () => {
  it("restores initially fitting panes after a result makes them scrollable", () => {
    const pane = new Pane()
    const parent = new Pane()
    pane.parentElement = parent
    vi.stubGlobal("HTMLElement", Pane)
    vi.stubGlobal("document", {
      querySelectorAll: () => [pane],
      scrollingElement: null,
    })
    const restore = captureScroll()
    pane.scrollHeight = 500
    pane.scrollTop = 250
    pane.scrollWidth = 300
    pane.scrollLeft = 100
    parent.scrollHeight = 600
    parent.scrollTop = 367
    restore()
    expect(pane.scrollTop).toBe(0)
    expect(pane.scrollLeft).toBe(0)
    expect(parent.scrollTop).toBe(0)
  })
})
