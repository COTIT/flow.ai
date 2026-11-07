import { afterEach, beforeEach, expect, it, vi } from "vitest"
import {
  discoverTools,
  getOperation,
  operationManifest,
  prepareAction,
} from "./catalog"
import { callBackend, voiceFailure } from "./server"

const uuid = "0193f6b0-1c2d-7e3f-8a9b-0c1d2e3f4a5b"
const unavailable = [
  ["products_sale_statics", "native_statistics_not_implemented"],
  ["products_linked_products", "native_links_not_implemented"],
  ["products_sync_external", "external_sync_not_implemented"],
] as const
const originals = unavailable.map(([id]) => ({ ...getOperation(id)! }))

beforeEach(() => {
  for (const [id, unavailableReason] of unavailable)
    Object.assign(getOperation(id)!, {
      availability: "unavailable",
      unavailableReason,
    })
})
afterEach(() => {
  unavailable.forEach(([id], index) => {
    const tool = getOperation(id)! as unknown as Record<string, unknown>
    delete tool.availability
    delete tool.unavailableReason
    Object.assign(tool, originals[index])
  })
  vi.unstubAllGlobals()
})

it.each(unavailable)(
  "keeps %s inventoried and explicit in discovery",
  (id, unavailableReason) => {
    expect(operationManifest.some((tool) => tool.id === id)).toBe(true)
    expect(getOperation(id)?.disposition).toBe("execute")
    const found = discoverTools(id, ["workspace_admin"])[0]!
    expect(found).toMatchObject({
      id,
      availability: "unavailable",
      unavailableReason,
    })
    expect(found.description).toContain("unavailable")
  }
)

it.each(unavailable)(
  "refuses preparing %s with safe501 before an action exists",
  async (id) => {
    let failure: unknown
    try {
      prepareAction(id, { path: { product: uuid } }, uuid)
    } catch (error) {
      failure = error
    }
    expect(failure).toMatchObject({ status: 501, code: "unavailable" })
    const response = voiceFailure(failure)
    expect(response.status).toBe(501)
    expect(await response.json()).toEqual({
      message: "This capability is unavailable in the current backend.",
    })
  }
)

it.each(unavailable)(
  "independently refuses %s before network access or placeholder results",
  async (id) => {
    const fetcher = vi.fn(async () =>
      Response.json(
        id === "products_sync_external"
          ? { message: "ok" }
          : {
              data:
                id === "products_linked_products"
                  ? { products: [] }
                  : {
                      units_sold: 0,
                      revenue: 0,
                      avg_monthly_sales: null,
                      last_sale: null,
                      customer_orders: 0,
                      last_order: null,
                      months_active: null,
                    },
            }
      )
    )
    vi.stubGlobal("fetch", fetcher)
    const tool = getOperation(id)!
    const result = await callBackend(
      {
        operationId: id,
        summary: tool.summary,
        method: tool.method,
        path: tool.path.replace("{workspace}", uuid).replace("{product}", uuid),
        write: tool.write,
        idempotent: tool.idempotent,
      },
      {
        token: "staff",
        workspaceId: uuid,
        apiUrl: "http://backend.test/v1",
        permissions: [],
        sessionId: "s1",
      },
      "a1"
    )
    expect(result.status).toBe("failed")
    expect(result.data).toBeUndefined()
    expect(fetcher).not.toHaveBeenCalled()
  }
)
