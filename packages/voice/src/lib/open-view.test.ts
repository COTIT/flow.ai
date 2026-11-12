import { describe, expect, it } from "vitest"

import { openViewTool } from "./open-view"

describe("open_flow_view client tool (P4)", () => {
  it("answers unavailable at once where the host opens no views (Office)", () => {
    expect(JSON.parse(openViewTool()({ view: "stock" }))).toMatchObject({
      status: "unavailable",
    })
  })
  it("passes the asked view to the host and returns its answer", () => {
    const asked: string[] = []
    const tool = openViewTool((view) => {
      asked.push(view)
      return { status: "opened", view: "stock", title: "Stock Watch" }
    })
    expect(JSON.parse(tool({ view: "stock" }))).toEqual({
      status: "opened",
      view: "stock",
      title: "Stock Watch",
    })
    expect(asked).toEqual(["stock"])
  })
  it("never throws: a failing host or a missing view answers plainly", () => {
    const tool = openViewTool(() => {
      throw new Error("router down")
    })
    expect(JSON.parse(tool({ view: "stock" }))).toEqual({ status: "failed" })
    const seen: string[] = []
    openViewTool(
      (view) => (seen.push(view), { status: "not_found", available: [] })
    )({})
    expect(seen).toEqual([""])
  })
})
