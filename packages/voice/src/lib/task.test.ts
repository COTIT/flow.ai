import { describe, expect, it } from "vitest"

import type { VoiceOutcome, VoiceReview } from "./client-contract"
import type { BatchProgress } from "./continuity"
import type { TranscriptRow } from "./helpers"
import {
  batchCounts,
  clarifyOf,
  receiptOf,
  stockAt,
  taskView,
  type TaskInput,
} from "./task"

const review = (over: Partial<VoiceReview> = {}): VoiceReview => ({
  ticket: "t1",
  summary: "Set stock levels",
  confirmationCode: "135790",
  confirmationPhrase: "Confirm action 135790",
  details: {},
  expiresAt: "2026-11-11T12:00:00Z",
  ...over,
})
const idle: TaskInput = {
  review: null,
  busy: false,
  thinking: false,
  progress: null,
  batchProgress: null,
  outcome: null,
  decided: null,
  clarifying: false,
}
const batch = (state: string, statuses: string[]): BatchProgress => ({
  id: "b1",
  state,
  nextIndex: 0,
  recoverable: false,
  steps: statuses.map((status) => ({ status })),
})

describe("taskView", () => {
  it("shows nothing when no task is active, and nothing for a plain read answer", () => {
    expect(taskView(idle)).toBeNull()
    expect(
      taskView({ ...idle, outcome: { status: "completed", data: [] } })
    ).toBeNull()
  })

  it("prepares only while a change is being prepared, not for every question", () => {
    expect(taskView({ ...idle, thinking: true })).toBeNull()
    expect(
      taskView({
        ...idle,
        progress: { status: "checking", steps: [], narration: "" },
      })
    ).toBeNull()
    expect(
      taskView({
        ...idle,
        progress: { status: "preparing_review", steps: [], narration: "" },
      })
    ).toEqual({ state: "preparing", milestone: "preparingReview" })
  })

  it("waits for input while a choice is open", () => {
    expect(taskView({ ...idle, thinking: true, clarifying: true })).toEqual({
      state: "waiting_input",
      milestone: "waitingInput",
    })
  })

  it("needs approval while a review waits, runs once approved", () => {
    expect(taskView({ ...idle, review: review() })).toEqual({
      state: "needs_approval",
      milestone: "waitingConfirmation",
    })
    expect(taskView({ ...idle, review: review(), busy: true })).toEqual({
      state: "running",
      milestone: "executing",
    })
  })

  it("counts a batch's real steps; no percentages", () => {
    const view = taskView({
      ...idle,
      review: review({ kind: "batch", batchId: "b1" }),
      busy: true,
      batchProgress: batch("running", [
        "completed",
        "completed",
        "running",
        "not_started",
      ]),
    })
    expect(view).toEqual({
      state: "running",
      milestone: "executing",
      counts: { total: 4, done: 2, failed: 0, unknown: 0, notRun: 2 },
    })
  })

  it("a paused batch is paused, with what did and did not run", () => {
    expect(
      taskView({
        ...idle,
        decided: review({ kind: "batch" }),
        outcome: { status: "partial" },
        batchProgress: batch("paused", ["completed", "not_started"]),
      })
    ).toEqual({
      state: "paused",
      milestone: "paused",
      counts: { total: 2, done: 1, failed: 0, unknown: 0, notRun: 1 },
    })
  })

  it.each([
    ["completed", "completed"],
    ["partial", "partial"],
    ["failed", "failed"],
    ["expired", "failed"],
    ["cancelled", "cancelled"],
    ["not_started", "cancelled"],
    ["paused", "paused"],
    ["queued", "unknown"],
    ["unknown", "unknown"],
    ["something-new", "unknown"],
  ])("a decided action's outcome %s shows as %s", (status, state) => {
    expect(
      taskView({ ...idle, decided: review(), outcome: { status } })?.state
    ).toBe(state)
  })

  it("an unknown outcome says it is checking, never that it worked", () => {
    expect(
      taskView({ ...idle, decided: review(), outcome: { status: "unknown" } })
    ).toEqual({ state: "unknown", milestone: "checking" })
  })
})

describe("batchCounts", () => {
  it("never counts a not-run, failed or unknown step as done", () => {
    expect(
      batchCounts(
        batch("partial", [
          "completed",
          "failed",
          "unknown",
          "queued",
          "cancelled",
          "not_started",
          "awaiting_review",
          "partial",
        ])
      )
    ).toEqual({ total: 8, done: 1, failed: 2, unknown: 2, notRun: 3 })
  })
})

const stockReview = review({
  operationId: "stock_set_levels",
  details: {
    target: { product: "p-500" },
    record: { type: "Product", label: "Raw Honey 500g" },
    current: {
      kind: "stock",
      locked: {
        id: "p-500",
        product_name: "Raw Honey 500g",
        slots: [
          {
            warehouse_id: "w1",
            warehouse_name: "Main store",
            bin_id: null,
            before: "14",
          },
        ],
      },
    },
    changes: { warehouses: [{ warehouse_id: "w1", stock_lvl: 150 }] },
  },
})
const done = (over: Partial<VoiceOutcome> & Record<string, unknown> = {}) =>
  ({ status: "completed", data: { message: "ok" }, actionId: "a-1", ...over }) as VoiceOutcome

describe("receiptOf", () => {
  it("a stock set shows the server's read-back, never the proposed value", () => {
    const receipt = receiptOf(stockReview, done(), { "w1:": "150" })
    expect(receipt).toEqual({
      status: "completed",
      operationId: "stock_set_levels",
      record: "Raw Honey 500g",
      reference: "a-1",
      readBack: { product: "p-500" },
      rows: [
        {
          label: "Main store",
          before: "14",
          after: "150",
          source: "read",
          slot: { warehouse: "w1", bin: null },
        },
      ],
    })
  })

  it("a read-back that disagrees with the proposal shows the server value", () => {
    const receipt = receiptOf(stockReview, done(), { "w1:": "149" })
    expect(receipt.rows[0]).toMatchObject({ after: "149", source: "read" })
  })

  it("before the read-back answers, or when it fails, no new value is shown", () => {
    expect(receiptOf(stockReview, done(), undefined).rows[0]).toEqual({
      label: "Main store",
      before: "14",
      after: null,
      source: "read",
      slot: { warehouse: "w1", bin: null },
    })
  })

  it("a stock change uses the API's own before and after", () => {
    const mutate = review({
      operationId: "stock_mutate",
      details: { record: { type: "Product", label: "Raw Honey 500g" } },
    })
    const receipt = receiptOf(
      mutate,
      done({
        data: {
          message: "ok",
          before: "14.00",
          after: "34.00",
          warehouse: { id: "w1", name: "Main store" },
        },
      }),
      undefined
    )
    expect(receipt.readBack).toBeNull()
    expect(receipt.rows).toEqual([
      { label: "Main store", before: "14", after: "34", source: "api" },
    ])
  })

  it.each(["unknown", "failed", "running", "cancelled"])(
    "a %s outcome never shows a new value or a read-back",
    (status) => {
      const receipt = receiptOf(stockReview, done({ status }), { "w1:": "150" })
      expect(receipt.status).toBe(status === "running" ? "unknown" : status)
      expect(receipt.readBack).toBeNull()
      expect(receipt.rows.every((row) => row.after === null)).toBe(true)
    }
  )

  it("the reference is the assistant action id, top level or in data", () => {
    expect(receiptOf(review(), done(), undefined).reference).toBe("a-1")
    expect(
      receiptOf(
        review(),
        { status: "completed", data: { actionId: "a-2" } },
        undefined
      ).reference
    ).toBe("a-2")
    expect(
      receiptOf(review(), { status: "completed" }, undefined).reference
    ).toBeNull()
  })

  it("other writes show no invented values", () => {
    expect(
      receiptOf(
        review({
          operationId: "orders_create",
          details: { record: { type: "Order", label: null } },
        }),
        done(),
        undefined
      ).rows
    ).toEqual([])
  })
})

describe("stockAt", () => {
  const row = (warehouse: string, bin: string | null, lot: string | null, level: string) => ({
    warehouse: { id: warehouse },
    bin: bin ? { id: bin } : null,
    lot: lot ? { id: lot } : null,
    stock_lvl: level,
  })
  it("reads the exact warehouse and bin row without a lot, as the review did", () => {
    const rows = [
      row("w1", null, null, "150.00"),
      row("w1", "b1", null, "9.00"),
      row("w1", null, "l1", "5.00"),
      row("w2", null, null, "1.50"),
    ]
    expect(stockAt(rows, "w1", null)).toBe("150")
    expect(stockAt(rows, "w1", "b1")).toBe("9")
    expect(stockAt(rows, "w2", null)).toBe("1.5")
    // No row is zero, as the review's own snapshot counts it.
    expect(stockAt(rows, "w3", null)).toBe("0")
  })
})

describe("clarifyOf", () => {
  const products = (count: number): TranscriptRow => ({
    id: "r1",
    role: "result",
    text: "",
    views: [
      {
        kind: "records",
        resource: "products",
        total: count,
        href: "/products",
        rows: Array.from({ length: count }, (_, i) => ({
          id: `p${i}`,
          label: `Raw Honey ${i}`,
          detail: `HNY-${i}`,
          status: null,
          money: null,
          stock: 14,
          href: `/products/p${i}`,
        })),
      },
    ],
  })
  const agent = (text: string): TranscriptRow => ({ id: "a", role: "agent", text })
  const user = (text: string): TranscriptRow => ({ id: "u", role: "user", text })

  it("offers a choice when the newest products answer is followed by a question", () => {
    const choice = clarifyOf([user("update honey"), products(3), agent("Which one did you mean?")])
    expect(choice?.resultId).toBe("r1")
    expect(choice?.rows.map((row) => row.id)).toEqual(["p0", "p1", "p2"])
  })

  it("offers nothing for one product, no question, or once the person answered", () => {
    expect(clarifyOf([products(1), agent("Which one?")])).toBeNull()
    expect(clarifyOf([products(3), agent("Here they are.")])).toBeNull()
    expect(clarifyOf([products(3), agent("Which one?"), user("the 500g")])).toBeNull()
    expect(clarifyOf([])).toBeNull()
  })
})
