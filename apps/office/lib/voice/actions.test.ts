import { beforeEach, expect, it, vi } from "vitest"
import type { PreparedAction } from "./catalog"

const mocks = vi.hoisted(() => ({
  spans: [] as string[],
  ledger: vi.fn(),
  target: vi.fn(),
  checkpoint: vi.fn(),
}))
vi.mock("./tracing", () => ({
  noteVoiceFailure: () => {},
  withVoiceTracing: async (name: string, action: () => Promise<unknown>) => {
    mocks.spans.push(name)
    return action()
  },
}))
vi.mock("./bridge", async (original) => ({
  ...(await original<typeof import("./bridge")>()),
  assistantRequest: mocks.ledger,
}))
vi.mock("./review-target", () => ({ reviewTarget: mocks.target }))
vi.mock("./workflow", async (original) => ({
  ...(await original<typeof import("./workflow")>()),
  checkpointReview: mocks.checkpoint,
}))
import { proposeAction } from "./actions"

const context = {
  token: "staff",
  workspaceId: "w1",
  permissions: ["workspace_admin"],
  apiUrl: "https://api.test/v1",
  sessionId: "s1",
  language: "en",
}
const action: PreparedAction = {
  operationId: "orders_create",
  summary: "Create order",
  method: "POST",
  path: "/workspace/w1/orders",
  body: { reference: "r" },
  write: true,
  idempotent: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  mocks.spans.length = 0
  vi.stubEnv("ASSISTANT_BRIDGE_SECRET", "a".repeat(32))
  mocks.ledger.mockResolvedValue({
    id: "01950000-0000-7000-8000-000000000001",
    session_id: "s1",
    operation_id: "orders_create",
    state: "proposed",
    expires_at: new Date(Date.now() + 60000).toISOString(),
  })
  mocks.target.mockResolvedValue({})
  mocks.checkpoint.mockResolvedValue(undefined)
})

it("traces the draft, its review target lookup and ticket signing as separate spans", async () => {
  const result = await proposeAction(action, context, new Map())
  expect(result.status).toBe("requires_confirmation")
  expect(mocks.spans).toEqual(["propose", "review_target", "review_ticket"])
})

it("products_create review shows the proposed name and prices next to the currency facts", async () => {
  const { reviewStep } = await import("./actions")
  const workspace = "01950000-0000-7000-8000-000000000009"
  const { prepareAction } = await import("./catalog")
  const { reviewTarget } =
    await vi.importActual<typeof import("./review-target")>("./review-target")
  mocks.target.mockImplementation(reviewTarget)
  const create = prepareAction(
    "products_create",
    { body: { name: "A", price: "220", gross_price: "200" } },
    workspace
  )
  const step = await reviewStep(
    create,
    { ...context, workspaceId: workspace },
    {
      id: "01950000-0000-7000-8000-000000000002",
      session_id: "s1",
      operation_id: "products_create",
      state: "proposed",
      expires_at: new Date(Date.now() + 60000).toISOString(),
      review_facts: {
        version: 1,
        operation_id: "products_create",
        facts: {
          kind: "product",
          priced: {
            product_id: null,
            name: "A",
            currency: { id: "c1", code: "USD", decimals: 2, default: true },
            vat_group: null,
            price: "220.0000",
            gross_price: "200.0000",
          },
        },
      },
    }
  )
  expect(step.details).toMatchObject({
    changes: { name: "A", price: "220", gross_price: "200" },
    current: {
      kind: "product",
      priced: {
        name: "A",
        currency: "USD",
        price: "220.0000",
        gross_price: "200.0000",
      },
    },
  })
})

it("currencies_create review names the new currency and lists what will be created", async () => {
  const { reviewStep } = await import("./actions")
  const workspace = "01950000-0000-7000-8000-000000000009"
  const { prepareAction } = await import("./catalog")
  const { reviewTarget } =
    await vi.importActual<typeof import("./review-target")>("./review-target")
  mocks.target.mockImplementation(reviewTarget)
  const body = {
    asset_type: "EUR",
    name: "Euro",
    symbol_prefix: "€",
    decimals: 2,
    default_currency: false,
    active: true,
  }
  const step = await reviewStep(
    prepareAction("currencies_create", { body }, workspace),
    { ...context, workspaceId: workspace },
    {
      id: "01950000-0000-7000-8000-000000000002",
      session_id: "s1",
      operation_id: "currencies_create",
      state: "proposed",
      expires_at: new Date(Date.now() + 60000).toISOString(),
    }
  )
  expect(step.details).toMatchObject({
    record: { label: "Euro" },
    current: {
      changes: [
        { field: "code", before: null, after: "EUR" },
        { field: "name", before: null, after: "Euro" },
        { field: "symbol", before: null, after: "€" },
        { field: "decimals", before: null, after: "2" },
      ],
    },
    changes: body,
  })
})
