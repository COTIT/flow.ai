import { describe, expect, it } from "vitest"
import type { TranscriptRow } from "../helpers"
import type { ResultView } from "@/lib/voice/views"
import { withView } from "./rows"

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

  it("keeps at most 200 rows like the rest of the transcript", () => {
    const rows = Array.from({ length: 200 }, (_, i): TranscriptRow => ({ id: `${i}`, role: "agent", text: "x" }))
    expect(withView(rows, records, "r")).toHaveLength(200)
  })
})
