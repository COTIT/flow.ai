import { beforeEach, expect, it, vi } from "vitest"

const mocks = vi.hoisted(() => ({ backend: vi.fn() }))
vi.mock("./server", async (original) => ({
  ...(await original<typeof import("./server")>()),
  callBackend: mocks.backend,
}))
vi.mock("./workflow", () => ({
  checkpointReview: vi.fn(),
  createReview: () => "ticket",
  verifyReview: () => ({ confirmationCode: "123456" }),
}))
import { restoreReview, type StoredAction } from "./actions"

const workspace = "01950000-0000-7000-8000-000000000009"
const productA = "01950000-0000-7000-8000-000000000001"
const productB = "01950000-0000-7000-8000-000000000002"
const context = {
  token: "staff",
  workspaceId: workspace,
  permissions: ["product.delete", "product.view"],
  apiUrl: "https://api.test/v1",
  sessionId: "session",
  language: "en",
}
function draft(product: string, owner = workspace): StoredAction {
  return {
    id: "action",
    session_id: "session",
    operation_id: "products_delete",
    state: "proposed",
    expires_at: new Date(Date.now() + 60000).toISOString(),
    proposal: {
      method: "DELETE",
      path: `/workspace/${owner}/products/${product}`,
    },
  }
}
beforeEach(() => {
  vi.clearAllMocks()
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "a".repeat(32))
  mocks.backend.mockResolvedValue({ status: "failed" })
})
it("shows different delete targets and the authenticated workspace in canonical review", async () => {
  const a = (await restoreReview(draft(productA), context)).review!
  const b = (await restoreReview(draft(productB), context)).review!
  expect(a.details).toMatchObject({ workspace, target: { product: productA } })
  expect(b.details).toMatchObject({ workspace, target: { product: productB } })
  expect(a.details).not.toEqual(b.details)
})
it("refuses another workspace before fetching a target label", async () => {
  await expect(
    restoreReview(draft(productA, productB), context)
  ).rejects.toThrow(/workspace/i)
  expect(mocks.backend).not.toHaveBeenCalled()
})
it("uses one authorized resource lookup and projects only safe identifying fields", async () => {
  mocks.backend.mockResolvedValue({
    status: "completed",
    data: {
      data: {
        id: productA,
        name: "Cocoa",
        title: "Harvest",
        reference: "COC-01",
        number: 12,
        password: "secret",
        price: 125,
        credentials: { token: "hidden" },
        description: "private",
      },
    },
  })
  const review = (await restoreReview(draft(productA), context)).review!
  expect(review).toMatchObject({
    operationId: "products_delete",
    method: "DELETE",
  })
  expect(review.details.record).toEqual({ type: "product", label: "Cocoa" })
  expect(mocks.backend).toHaveBeenCalledTimes(1)
  expect(mocks.backend.mock.calls[0]![0]).toMatchObject({
    operationId: "products_get",
    write: false,
    path: `/workspace/${workspace}/products/${productA}`,
  })
  expect(JSON.stringify(review)).not.toContain("secret")
  expect(JSON.stringify(review)).not.toContain("hidden")
})
it("keeps bound IDs without reading when the staff member cannot view the resource", async () => {
  const review = (
    await restoreReview(draft(productA), {
      ...context,
      permissions: ["product.delete"],
    })
  ).review!
  expect(review.details).toMatchObject({
    workspace,
    target: { product: productA },
  })
  expect(review.details.record).toEqual({ type: "product", label: null })
  expect(mocks.backend).not.toHaveBeenCalled()
})
it("does not attach a name from a mismatched backend resource", async () => {
  mocks.backend.mockResolvedValue({
    status: "completed",
    data: { data: { id: productB, name: "Wrong product" } },
  })
  const review = (await restoreReview(draft(productA), context)).review!
  expect(review.details.record).toEqual({ type: "product", label: null })
})
it("includes every nested route identity in the review", async () => {
  const ids = {
    warehouse: productA,
    section: productB,
    row: "01950000-0000-7000-8000-000000000003",
    shelf: "01950000-0000-7000-8000-000000000004",
    bin: "01950000-0000-7000-8000-000000000005",
  }
  const nested: StoredAction = {
    ...draft(productA),
    operation_id: "warehouse_bins_delete",
    proposal: {
      method: "DELETE",
      path: `/workspace/${workspace}/warehouses/${ids.warehouse}/sections/${ids.section}/rows/${ids.row}/shelves/${ids.shelf}/bins/${ids.bin}`,
    },
  }
  const review = (
    await restoreReview(nested, {
      ...context,
      permissions: ["workspace_admin"],
    })
  ).review!
  expect(review.details.target).toEqual(ids)
  expect(mocks.backend).toHaveBeenCalledTimes(1)
  expect(mocks.backend.mock.calls[0]![0]).toMatchObject({
    operationId: "warehouse_bins_get",
    method: "GET",
    write: false,
  })
})
it("keeps bound identities when a resource lookup throws", async () => {
  mocks.backend.mockRejectedValue(new Error("backend unavailable"))
  const review = (await restoreReview(draft(productA), context)).review!
  expect(review.details).toMatchObject({
    workspace,
    target: { product: productA },
  })
  expect(review.details.record).toEqual({ type: "product", label: null })
})

it("names a customer by company, alias or first contact and its type in the user's language", async () => {
  const relation = "01950000-0000-7000-8000-000000000007"
  const action: StoredAction = {
    ...draft(productA),
    operation_id: "relations_delete",
    proposal: { method: "DELETE", path: `/workspace/${workspace}/relations/${relation}` },
  }
  const staff = { ...context, permissions: ["workspace_admin"] }
  for (const [record, label] of [
    [{ company_name: "Acme BV", alias: "Acme" }, "Acme BV"],
    [{ company_name: " ", alias: "Kendall" }, "Kendall"],
    [{ company_name: null, alias: null, relationships: { contacts: [{ full_name: "Kendall Moreau" }] } }, "Kendall Moreau"],
    [{ relation_nr: "C-0042" }, "C-0042"],
  ] as const) {
    mocks.backend.mockResolvedValueOnce({ status: "completed", data: { data: { id: relation, ...record } } })
    const review = (await restoreReview(action, staff)).review!
    expect(review.details.record).toEqual({ type: "customer", label })
  }
  mocks.backend.mockResolvedValueOnce({ status: "completed", data: { data: { id: relation, alias: "Kendall" } } })
  const dutch = (await restoreReview(action, { ...staff, language: "nl" })).review!
  expect(dutch.details.record).toEqual({ type: "klant", label: "Kendall" })
})
it("falls back to the API's reviewed delete label and lifts impact and warnings for the card", async () => {
  const action: StoredAction = {
    ...draft(productA),
    review_facts: {
      version: 1,
      operation_id: "products_delete",
      facts: { kind: "product", locked: { id: productA, label: "AFM Stickers" }, impact: { effect: "archive", variants: 2, stock_rows: 1 } },
    },
  }
  const review = (await restoreReview(action, { ...context, permissions: ["product.delete"] })).review!
  expect(review.details.record).toEqual({ type: "product", label: "AFM Stickers" })
  expect(review.details.impact).toEqual({ effect: "archive", variants: 2, stock_rows: 1 })
  // Record and impact already describe a delete; the raw locked snapshot would only repeat its id.
  expect(review.details.current).toBeUndefined()
  expect(review.details.warnings).toBeUndefined()
})
