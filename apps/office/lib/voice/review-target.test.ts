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
  expect(review.details.record).toEqual({
    id: productA,
    name: "Cocoa",
    title: "Harvest",
    reference: "COC-01",
    number: 12,
  })
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
  expect(review.details.record).toBeUndefined()
  expect(mocks.backend).not.toHaveBeenCalled()
})
it("does not attach a name from a mismatched backend resource", async () => {
  mocks.backend.mockResolvedValue({
    status: "completed",
    data: { data: { id: productB, name: "Wrong product" } },
  })
  const review = (await restoreReview(draft(productA), context)).review!
  expect(review.details.record).toBeUndefined()
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
  expect(review.details.record).toBeUndefined()
})
