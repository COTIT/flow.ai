import { describe, expect, it } from "vitest"
import type { TranscriptRow } from "./helpers"
import type { ResultView } from "./views"
import { replaceView, withView } from "./rows"

const sales = (from: string): ResultView => ({ kind: "sales", from, to: from, source: "till", rows: [] })
const records: ResultView = { kind: "records", resource: "orders", total: 0, href: "/orders", rows: [] }
const user: TranscriptRow = { id: "u", role: "user", text: "compare" }

describe("withView", () => {
  it("puts sales answers of one question into one chart", () => {
    const one = withView([user], sales("a"), "r1")
    const two = withView(one, sales("b"), "r2")
    expect(two).toHaveLength(2)
    expect(two[1]).toMatchObject({ id: "r1", role: "result", views: [sales("a"), sales("b")] })
  })

  it("a new question or another kind of picture starts a new row", () => {
    const one = withView([user], sales("a"), "r1")
    expect(withView([...one, { ...user, id: "u2" }], sales("b"), "r2")).toHaveLength(4)
    expect(withView(one, records, "r2").at(-1)).toMatchObject({ id: "r2", views: [records] })
  })

  it("remembers which read a new picture came from", () => {
    expect(withView([user], records, "r1", "orders_list").at(-1)).toMatchObject({ id: "r1", operation: "orders_list" })
    expect(withView([user], records, "r1").at(-1)).not.toHaveProperty("operation")
  })

  it("keeps at most 200 rows like the rest of the transcript", () => {
    const rows = Array.from({ length: 200 }, (_, i): TranscriptRow => ({ id: `${i}`, role: "agent", text: "x" }))
    expect(withView(rows, records, "r")).toHaveLength(200)
  })
})

describe("replaceView", () => {
  const first = withView([user], sales("a"), "r1", "orders_sales_summary")
  const later = [...first, { id: "u2", role: "user" as const, text: "as a table" }]

  it("changes the named picture where it is, keeping its id and place", () => {
    const table = { ...sales("b"), presentation: "table" as const }
    const out = replaceView(later, "r1", table, "orders_sales_summary", "r9")
    expect(out).toHaveLength(3)
    expect(out[1]).toEqual({ id: "r1", role: "result", text: "", views: [table], operation: "orders_sales_summary" })
  })

  it("adds a new picture when the named one is not on screen, and removes nothing", () => {
    const out = replaceView(later, "gone", records, "orders_list", "r9")
    expect(out).toHaveLength(4)
    expect(out.slice(0, 3)).toEqual(later)
    expect(out[3]).toMatchObject({ id: "r9", views: [records], operation: "orders_list" })
  })

  it("never turns another kind of row into a picture", () => {
    expect(replaceView(later, "u2", records, "orders_list", "r9")).toHaveLength(4)
  })
})
