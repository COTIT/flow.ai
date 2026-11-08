import { createHash } from "node:crypto"
import { beforeEach, describe, expect, it, vi } from "vitest"
const mocks = vi.hoisted(() => ({
  ledger: vi.fn(),
  batch: vi.fn(),
  single: vi.fn(),
}))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: mocks.ledger,
}))
vi.mock("./batches", async (original) => ({
  ...(await original<typeof import("./batches")>()),
  proposeBatch: mocks.batch,
}))
vi.mock("./actions", async (original) => ({
  ...(await original<typeof import("./actions")>()),
  proposeAction: mocks.single,
}))
import {
  importPrices,
  notIncludedFromModel,
  withNotIncluded,
} from "./file-import"
import type { VoiceContext } from "./server"

const handle = "01950000-0000-7000-8000-0000000000aa"
const p1 = "01950000-0000-7000-8000-000000000001"
const p2 = "01950000-0000-7000-8000-000000000002"
const context = {
  token: "t",
  workspaceId: "01950000-0000-7000-8000-000000000009",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "s",
  language: "en",
} as unknown as VoiceContext
const csv = "Product,Price\nRice,12.50\nBeans,3\nMaize,x\n"
const blob = (text: string) => new Blob([text], { type: "text/csv" })
const stored = (text: string, extra = {}) => ({
  id: handle,
  state: "uploaded",
  media_type: "text/csv",
  size_bytes: Buffer.byteLength(text),
  sha256: createHash("sha256").update(text).digest("hex"),
  ...extra,
})
const files = (text = csv) => new Map([[handle, blob(text)]])
const args = (extra = {}) => ({ attachments: { sheet: [handle] }, ...extra })
const review = { status: "requires_confirmation", review: { details: {} } }

beforeEach(() => {
  vi.clearAllMocks()
  mocks.batch.mockResolvedValue(structuredClone(review))
  mocks.single.mockResolvedValue(structuredClone(review))
})

function api(rows: unknown[], upload = stored(csv)) {
  mocks.ledger.mockImplementation(
    async (_c, path: string, method?: string, body?: unknown) => {
      if (path === `/sessions/s/uploads/${handle}`) return upload
      if (path === "/sessions/s/price-import" && method === "POST") {
        api.sent = body
        return rows
      }
      throw new Error("unexpected " + path)
    }
  )
}
api.sent = undefined as unknown

describe("importPrices", () => {
  it("turns matched rows into one batch and lists the rest as not included", async () => {
    api([
      {
        line: 2,
        status: "matched",
        product_id: p1,
        name: "Rice",
        price: "12.50",
        current: "10.00",
      },
      {
        line: 3,
        status: "matched",
        product_id: p2,
        name: "Beans",
        price: "3",
        current: "2.00",
      },
      {
        line: 4,
        status: "unreadable",
        product_id: p2,
        name: "Maize",
        current: "1.00",
      },
    ])
    const outcome = await importPrices(args(), context, files())
    expect(api.sent).toEqual({
      rows: [
        { line: 2, key: "Rice", price: "12.50" },
        { line: 3, key: "Beans", price: "3" },
        { line: 4, key: "Maize", price: "x" },
      ],
    })
    const [calls, , sent, max] = mocks.batch.mock.calls[0]!
    expect(calls).toEqual([
      {
        operationId: "products_update",
        arguments: { path: { product: p1 }, body: { price: "12.50" } },
      },
      {
        operationId: "products_update",
        arguments: { path: { product: p2 }, body: { price: "3" } },
      },
    ])
    expect(sent.size).toBe(0)
    expect(max).toBe(50)
    expect(outcome.review!.details).toEqual({
      rows: { from: 1, to: 3, total: 3 },
      notIncluded: [{ line: 4, text: "Maize", reason: "unreadable" }],
    })
    expect(outcome.data).toMatchObject({ rows: { from: 1, to: 3, total: 3 } })
  })

  it("uses one review card for a single matched row and no review for none", async () => {
    api([
      {
        line: 2,
        status: "matched",
        product_id: p1,
        name: "Rice",
        price: "12.50",
        current: "1",
      },
    ])
    await importPrices(args(), context, files())
    expect(mocks.single).toHaveBeenCalledOnce()
    expect(mocks.single.mock.calls[0]![0]).toMatchObject({
      operationId: "products_update",
      body: { price: "12.50" },
    })
    api([
      { line: 2, status: "not_found" },
      { line: 3, status: "ambiguous", candidates: ["Beans", "beans"] },
      {
        line: 4,
        status: "unchanged",
        product_id: p1,
        name: "Maize",
        current: "1",
      },
    ])
    const outcome = await importPrices(args(), context, files())
    expect(mocks.batch).not.toHaveBeenCalled()
    expect(outcome.status).toBe("completed")
    expect(outcome.review).toBeUndefined()
    expect(outcome.data).toMatchObject({
      notIncluded: [
        { line: 2, text: "Rice", reason: "not_found" },
        {
          line: 3,
          text: "Beans",
          reason: "ambiguous",
          candidates: ["Beans", "beans"],
        },
        { line: 4, text: "Maize", reason: "unchanged" },
      ],
    })
  })

  it("pages fifty rows at a time and says where the next page starts", async () => {
    const big =
      "Name,Price\n" +
      Array.from({ length: 60 }, (_, i) => `P${i},${i + 1}`).join("\n")
    api(
      Array.from({ length: 10 }, (_, i) => ({
        line: 52 + i,
        status: "not_found",
      })),
      stored(big)
    )
    const first = await importPrices(args(), context, files(big))
    expect((api.sent as { rows: unknown[] }).rows).toHaveLength(50)
    expect(first.data).toMatchObject({
      rows: { from: 1, to: 50, total: 60, next: 51 },
    })
    await importPrices(args({ fromRow: 51 }), context, files(big))
    expect((api.sent as { rows: { line: number }[] }).rows[0]!.line).toBe(52)
  })

  it("asks for columns when headers are unclear", async () => {
    const text = "A,B\nRice,1\n"
    api([], stored(text))
    const outcome = await importPrices(args(), context, files(text))
    expect(outcome.status).toBe("needs_input")
    expect(outcome.data).toEqual({ columns: ["A", "B"] })
    expect(mocks.ledger).not.toHaveBeenCalledWith(
      context,
      "/sessions/s/price-import",
      "POST",
      expect.anything()
    )
  })

  it("refuses changed bytes, missing files and non-sheet uploads", async () => {
    api([], stored(csv))
    await expect(
      importPrices(args(), context, files(csv + "x"))
    ).rejects.toMatchObject({ status: 409 })
    await expect(importPrices(args(), context, new Map())).rejects.toThrow(
      /again/
    )
    api([], stored(csv, { media_type: "application/pdf" }))
    await expect(importPrices(args(), context, files())).rejects.toMatchObject({
      status: 409,
    })
    await expect(
      importPrices(
        { attachments: { sheet: [handle, handle] } },
        context,
        files()
      )
    ).rejects.toThrow(/one price list/)
  })
})

describe("model-reported unclear lines", () => {
  it("keeps bounded plain text and never becomes an action", () => {
    expect(
      notIncludedFromModel([
        { line: 3, seen: "Su?ar 5 kg", note: "quantity smudged" },
      ])
    ).toEqual([
      {
        line: 3,
        text: "Su?ar 5 kg",
        reason: "unclear",
        note: "quantity smudged",
      },
    ])
    expect(notIncludedFromModel(undefined)).toEqual([])
    expect(() => notIncludedFromModel("x")).toThrow()
    expect(() => notIncludedFromModel(Array(21).fill({ seen: "x" }))).toThrow(
      /20/
    )
    expect(() => notIncludedFromModel([{ seen: "x".repeat(121) }])).toThrow(
      /120/
    )
  })
  it("adds them to a review without touching the steps", () => {
    const outcome = {
      status: "requires_confirmation",
      review: { details: { a: 1 } },
    }
    withNotIncluded(outcome as never, [{ text: "x", reason: "unclear" }])
    expect(outcome.review.details).toEqual({
      a: 1,
      notIncluded: [{ text: "x", reason: "unclear" }],
    })
    const plain = { status: "completed" }
    expect(withNotIncluded(plain as never, [])).toEqual({ status: "completed" })
  })
})
