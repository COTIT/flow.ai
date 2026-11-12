import { describe, expect, it } from "vitest"
import {
  activityPending,
  activityState,
  composerReady,
  displayReferences,
  displayText,
  operationLabel,
  notIncludedRows,
  reviewRows,
  sessionPhase,
} from "./presentation"

describe("assistant activity", () => {
  it("keeps a sent request pending until a reply, and distinguishes execution from approval", () => {
    expect(activityState({ connected: true, thinking: true })).toBe("thinking")
    expect(activityState({ connected: true, muted: true })).toBe("muted")
    expect(activityState({ connected: true, muted: true, hold: true })).toBe(
      "holdToTalk"
    )
    expect(activityState({ connected: true, hold: true })).toBe("listening")
    expect(
      activityState({ connected: true, thinking: true, requesting: true })
    ).toBe("executing")
    expect(
      activityState({
        connected: true,
        thinking: true,
        requesting: true,
        review: true,
      })
    ).toBe("awaiting_review")
    expect(activityState({ connected: true, review: true, busy: true })).toBe(
      "executing"
    )
    expect(
      activityState({ connected: true, thinking: true, error: "Failed" })
    ).toBe("failed")
  })
  it("never turns queued, partial, or unverified native results into completion", () => {
    for (const status of [
      "queued",
      "partial",
      "unknown",
      "paused",
      "completed",
    ])
      expect(activityState({ connected: true, outcome: status })).toBe(status)
    expect(activityState({ connected: true })).toBe("listening")
    expect(activityState({ connected: true, textOnly: true })).toBe("ready")
    expect(activityState({})).toBe("idle")
  })
  it("preserves native receipts when the conversation channel reports an error", () => {
    for (const outcome of [
      "queued",
      "partial",
      "unknown",
      "completed",
      "cancelled",
    ])
      expect(activityState({ error: "Connection lost", outcome })).toBe(outcome)
  })
})

describe("an expired review", () => {
  it("reads as expired, never as a failure (P8-2: the status line, like the orb, stays calm)", () => {
    const expiry = "This review expired. Ask again for a fresh proposal."
    expect(activityState({ connected: true, error: expiry, expired: true })).toBe("expired")
    expect(activityState({ connected: true, error: "Network down" })).toBe("failed")
  })
})

describe("readable approval presentation", () => {
  const id = "01950000-0000-7000-8000-000000000001"
  const other = "01950000-0000-7000-8000-000000000002"
  it("uses existing authorized record names without changing signed data", () => {
    const details = {
      target: { product: id },
      record: { id, name: "Cocoa", reference: "COC-01" },
      changes: { quantity: "2.00", total: "€1.25" },
    }
    const original = JSON.stringify(details)
    const references = displayReferences(details)
    expect(displayText(id, references, (n) => `Reference ${n}`)).toContain(
      "Cocoa"
    )
    expect(
      displayText(
        `Target ${id}, related ${other}`,
        references,
        (n) => `Reference ${n}`
      )
    ).toBe("Target Cocoa · COC-01, related Reference 2")
    expect(JSON.stringify(details)).toBe(original)
  })
  it("gives unnamed references stable translated labels instead of literal IDs", () => {
    const references = displayReferences({
      target: { product: id },
      changes: { warehouse_id: other },
    })
    expect(displayText(id, references, (n) => `Verwijzing ${n}`)).toBe(
      "Verwijzing 1"
    )
    expect(displayText(other, references, (n) => `Verwijzing ${n}`)).toBe(
      "Verwijzing 2"
    )
    expect(
      displayText(
        "Order ORD-2026-14 · €1.25",
        references,
        (n) => `Reference ${n}`
      )
    ).toBe("Order ORD-2026-14 · €1.25")
  })
  it("supplies translatable operation meaning, including destructive methods", () => {
    expect(operationLabel("orders_create", "POST")).toEqual({
      action: "create",
      resource: "orders",
    })
    expect(operationLabel("invoices_send", "POST")).toEqual({
      action: "send",
      resource: "invoices",
    })
    expect(operationLabel("app_keys_revoke", "DELETE")).toEqual({
      action: "delete",
      resource: "app_keys",
    })
    expect(operationLabel("new_resource_custom", "PATCH")).toEqual({
      action: "update",
      resource: "record",
    })
    for (const [id, action] of [
      ["storefront_design_edit", "update"],
      ["storefront_design_restore", "restore"],
      ["storefront_design_publish", "publish"],
    ] as const)
      expect(operationLabel(id, "POST")).toEqual({
        action,
        resource: "storefront_design",
      })
  })
})

describe("assistant screen split", () => {
  it("shows the thinking line only while the assistant works", () => {
    for (const state of [
      "connecting",
      "upgrading",
      "thinking",
      "executing",
      "uploading",
      "checking",
    ])
      expect(activityPending(state)).toBe(true)
    for (const state of ["listening", "speaking", "awaiting_review", "failed"])
      expect(activityPending(state)).toBe(false)
  })
  it("arms send for staged files and not for files already sent", () => {
    expect(composerReady([])).toBe(false)
    expect(composerReady([{ state: "submitted" }])).toBe(false)
    expect(composerReady([{ state: "submitted" }, { state: "ready" }])).toBe(
      true
    )
  })
})

describe("review rows", () => {
  const base = {
    ticket: "t",
    summary: "Set stock",
    confirmationCode: "1",
    confirmationPhrase: "Approve",
    expiresAt: "2026-11-08T00:00:00Z",
  }
  it("lists each reviewed field in order and hides internal fields", () => {
    const rows = reviewRows({
      ...base,
      details: {
        product: "Honey",
        actionId: "a",
        ticket: "secret",
        quantity: 120,
        bodySha256: "x",
      },
    })
    expect(rows).toEqual([
      { kind: "field", id: "product", key: "product", value: "Honey" },
      { kind: "field", id: "quantity", key: "quantity", value: 120 },
    ])
  })
  it("lists each batch step in order, numbered from one", () => {
    const step = (actionId: string) => ({
      actionId,
      summary: actionId,
      details: {},
    })
    const rows = reviewRows({
      ...base,
      kind: "batch",
      details: { product: "ignored" },
      steps: [step("a1"), step("a2"), step("a3")],
    })
    expect(rows.map((row) => [row.kind, row.id])).toEqual([
      ["step", "a1"],
      ["step", "a2"],
      ["step", "a3"],
    ])
    expect(rows.map((row) => row.kind === "step" && row.number)).toEqual([
      1, 2, 3,
    ])
  })
  it("falls back to fields for a batch without steps, and to nothing when all fields are internal", () => {
    expect(
      reviewRows({ ...base, kind: "batch", steps: [], details: { a: 1 } })
    ).toHaveLength(1)
    expect(reviewRows({ ...base, details: { ticket: "x" } })).toEqual([])
  })
  it("keeps rows left out of a review apart from the fields, with known reasons only", () => {
    const review = {
      ...base,
      details: {
        product: "Honey",
        rows: { from: 1, to: 3, total: 3 },
        notIncluded: [
          { line: 4, text: "Maize", reason: "unreadable" },
          { text: "Su?ar", reason: "unclear", note: "smudged" },
          { line: 5, text: "x", reason: "<script>" },
          "junk",
        ],
      },
    }
    expect(reviewRows(review).map((row) => row.id)).toEqual(["product"])
    expect(notIncludedRows(review)).toEqual([
      { line: 4, text: "Maize", reason: "unreadable" },
      { text: "Su?ar", reason: "unclear", note: "smudged" },
      { line: 5, text: "x", reason: "other" },
    ])
    expect(notIncludedRows({ ...base, details: {} })).toEqual([])
  })
})

describe("session phase", () => {
  it("is ready before any session, live while starting or connected", () => {
    expect(sessionPhase({})).toEqual({ phase: "ready" })
    expect(sessionPhase({ starting: true })).toEqual({ phase: "live" })
    expect(sessionPhase({ connected: true })).toEqual({ phase: "live" })
    expect(sessionPhase({ upgrading: true, endedByUser: true })).toEqual({
      phase: "live",
    })
  })

  it("is ended after the person ends it or the provider drops it, never while live", () => {
    expect(sessionPhase({ endedByUser: true })).toEqual({
      phase: "ended",
      reason: "user",
    })
    expect(sessionPhase({ unfinished: "dropped" })).toEqual({
      phase: "ended",
      reason: "dropped",
    })
    expect(
      sessionPhase({ unfinished: "dropped", endedByUser: true })
    ).toEqual({ phase: "ended", reason: "dropped" })
    // A stalled answer is still a live session.
    expect(sessionPhase({ connected: true, unfinished: "stalled" })).toEqual({
      phase: "live",
    })
  })
})
