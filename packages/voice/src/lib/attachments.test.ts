import { describe, expect, it } from "vitest"
import {
  attachmentProblem,
  sheetMime,
  mergeTranscript,
  outcomeForAgent,
  fieldName,
  splitLinks,
  matchesIssuedConversation,
  referenceSize,
  taskPresentation,
} from "./helpers"

describe("temporary voice references", () => {
  it("shows exact reference units and distinguishes unrun batch steps from uncertainty", () => {
    expect(referenceSize(589, "en")).toBe("589 B")
    expect(referenceSize(1536, "nl")).toBe("1,5 KiB")
    expect(referenceSize(10 * 1024 * 1024, "en")).toBe("10 MiB")
    expect(taskPresentation("awaiting_review", "paused")).toBe("not_run")
    expect(taskPresentation("awaiting_review", "proposed")).toBe(
      "awaiting_review"
    )
    expect(taskPresentation("queued")).toBe("queued")
    expect(taskPresentation("not_started", "paused")).toBe("not_run")
    expect(taskPresentation("unexpected")).toBe("unknown")
  })
  it("opens capabilities only for the server-issued provider conversation", () => {
    expect(matchesIssuedConversation("issued", "issued")).toBe(true)
    expect(matchesIssuedConversation("issued", "different")).toBe(false)
    expect(matchesIssuedConversation(undefined, "different")).toBe(false)
    expect(matchesIssuedConversation("", "")).toBe(false)
  })
  it("rejects uninspectable input and applies canonical size/count limits", () => {
    expect(attachmentProblem({ type: "image/svg+xml", size: 20 }, 0)).toBe(
      "type"
    )
    expect(attachmentProblem({ type: "text/plain", size: 12 }, 0)).toBe("type")
    expect(attachmentProblem({ type: "image/png", size: 0 }, 0)).toBe("empty")
    expect(
      attachmentProblem(
        { type: "application/pdf", size: 10 * 1024 * 1024 + 1 },
        0
      )
    ).toBe("size")
    expect(attachmentProblem({ type: "text/plain", size: 12 }, 10)).toBe(
      "count"
    )
    expect(attachmentProblem({ type: "image/webp", size: 12 }, 0)).toBeNull()
  })
  it("accepts .xlsx and .csv price lists up to 2 MiB, whatever label the browser gives", () => {
    const xlsx =
      "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
    expect(sheetMime({ type: "", name: "Prices.CSV" })).toBe("text/csv")
    expect(sheetMime({ type: "application/vnd.ms-excel", name: "p.csv" })).toBe(
      "text/csv"
    )
    expect(sheetMime({ type: "", name: "p.xlsx" })).toBe(xlsx)
    expect(
      sheetMime({ type: "application/vnd.ms-excel", name: "p.xls" })
    ).toBeNull()
    expect(sheetMime({ type: "image/png", name: "p.png" })).toBeNull()
    const sheet = { type: "", name: "p.csv" }
    expect(attachmentProblem({ ...sheet, size: 2 * 1024 * 1024 }, 0)).toBeNull()
    expect(attachmentProblem({ ...sheet, size: 2 * 1024 * 1024 + 1 }, 0)).toBe(
      "sheetSize"
    )
    expect(attachmentProblem({ type: "", name: "p.xls", size: 5 }, 0)).toBe(
      "type"
    )
  })
  it("replaces corrected response by stable event without deleting other turns", () => {
    const first = mergeTranscript([], {
      id: "agent-1",
      role: "agent",
      text: "Amount 15",
    })
    const next = mergeTranscript(first, {
      id: "user-2",
      role: "user",
      text: "No, 50",
    })
    expect(
      mergeTranscript(next, { id: "agent-1", role: "agent", text: "Amount 50" })
    ).toEqual([
      { id: "agent-1", role: "agent", text: "Amount 50" },
      { id: "user-2", role: "user", text: "No, 50" },
    ])
  })
  it("drops an agent line repeated in the same turn, keeps it in a later turn", () => {
    const line = { role: "agent" as const, text: "I'm checking the stock." }
    let rows = mergeTranscript([], {
      id: "user-1",
      role: "user",
      text: "Check it",
    })
    rows = mergeTranscript(rows, { ...line, id: "agent-1" })
    rows = mergeTranscript(rows, {
      ...line,
      id: "agent-2",
      text: " i'm checking the  stock. ",
    })
    expect(rows.map((row) => row.id)).toEqual(["user-1", "agent-1"])
    rows = mergeTranscript(rows, { id: "user-2", role: "user", text: "Again" })
    rows = mergeTranscript(rows, { ...line, id: "agent-3" })
    expect(rows.map((row) => row.id)).toEqual([
      "user-1",
      "agent-1",
      "user-2",
      "agent-3",
    ])
  })
})

describe("record links", () => {
  it("turns only office and https markdown links into links", () => {
    expect(
      splitLinks(
        "Created [ORD-0229](/orders/abc). See [docs](https://genesy.app/x) not [bad](javascript:alert(1))."
      )
    ).toEqual([
      { text: "Created " },
      { text: "ORD-0229", href: "/orders/abc" },
      { text: ". See " },
      { text: "docs", href: "https://genesy.app/x" },
      { text: " not [bad](javascript:alert(1))." },
    ])
    expect(splitLinks("plain")).toEqual([{ text: "plain" }])
  })
  it("tells the agent record names, never the links", () => {
    expect(
      outcomeForAgent({
        status: "completed",
        links: [
          { label: "ORD-0229", href: "/orders/a" },
          { label: null, href: "/orders/b" },
        ],
      })
    ).toEqual({ status: "completed", records: ["ORD-0229"] })
    expect(outcomeForAgent({ status: "failed" })).toEqual({ status: "failed" })
    // The picture is for the screen; the agent keeps the data it already gets.
    expect(
      outcomeForAgent({ status: "completed", data: [1], view: { kind: "sales" } })
    ).toMatchObject({ status: "completed", data: [1] })
    expect(
      outcomeForAgent({ status: "completed", data: [1], view: { kind: "sales" } })
    ).not.toHaveProperty("view")
  })
  it("says how many points the picture shows", () => {
    const view = {
      kind: "sales_daily",
      from: "2026-11-03",
      to: "2026-11-06",
      time_zone: "UTC",
      currencies: [
        { currency: "TZS", decimals: 0, days: [{ date: "2026-11-03", gross: 1, net: 1, orders: 1 }, { date: "2026-11-04", gross: 0, net: 0, orders: 0 }] },
        { currency: "USD", decimals: 2, days: [{ date: "2026-11-03", gross: 5, net: 5, orders: 1 }] },
      ],
    }
    expect(outcomeForAgent({ status: "completed", view }).on_screen).toMatchObject({ kind: "chart", points: 3 })
    const records = { kind: "records", resource: "orders", total: 9, href: "/orders", rows: [{}, {}] }
    expect(outcomeForAgent({ status: "completed", view: records }).on_screen).toMatchObject({ kind: "list", points: 2 })
  })

  it("knows a picture is on screen, so it summarises instead of reading every point", () => {
    const chart = outcomeForAgent({ status: "completed", data: [1, 2, 3], view: { kind: "sales_daily" } })
    expect(chart.on_screen).toEqual({
      kind: "chart",
      points: 0,
      instruction: expect.stringMatching(/one or two sentences.*never read/i),
    })
    expect(outcomeForAgent({ status: "completed", view: { kind: "records" } }).on_screen?.kind).toBe("list")
    // Nothing shown, nothing claimed.
    expect(outcomeForAgent({ status: "completed", data: [1] })).not.toHaveProperty("on_screen")
    expect(outcomeForAgent({ status: "completed", view: { kind: "unknown_kind" } })).not.toHaveProperty("on_screen")
  })
})

describe("field names without a label", () => {
  it("read in sentence case, like the labelled ones", () => {
    expect(fieldName("purchase_price")).toBe("Purchase price")
    expect(fieldName("vatGroup")).toBe("Vat group")
    expect(fieldName("stock_rows")).toBe("Stock rows")
  })
})
