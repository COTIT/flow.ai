import { describe, expect, it } from "vitest"
import {
  attachmentProblem,
  mergeTranscript,
  outcomeForAgent,
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
  })
})
