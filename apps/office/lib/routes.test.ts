import { expect, it, vi } from "vitest"

vi.mock("@genesy/api-client/auth", () => ({ useSession: vi.fn() }))
vi.mock("@genesy/api-client/endpoints/users", () => ({ useUsersMe: vi.fn() }))

it("each office route keeps the right its page list needs, readable on the server", async () => {
  const { rightFor } = await import("./routes")
  expect(rightFor("/products")).toBe("product.list")
  expect(rightFor("/products/categories")).toBe("product.list")
  expect(rightFor("/stock/negative")).toBe("stock.list")
  expect(rightFor("/till/sales")).toBe("cashregister.list")
  expect(rightFor("/settings/webhooks")).toBe("workspace_admin")
  expect(rightFor("/settings/storefront")).toBe("workspace_admin")
  expect(rightFor("/settings/payments")).toBe("workspace_admin")
  expect(rightFor("/payments")).toBe("payment.read")
  expect(rightFor("/supply/links")).toBe("workspace_admin")
  expect(rightFor("/supply/transfers")).toBeUndefined()
  expect(rightFor("/")).toBeUndefined()
  expect(rightFor("/productsx")).toBeUndefined()
})

it("the client helper still answers with the same rule", async () => {
  const can = await import("./can")
  const routes = await import("./routes")
  expect(can.rightFor).toBe(routes.rightFor)
})

it.each(["invoice.list", "order.list", "product.list"])(
  "payments navigation checks %s",
  async (right) => {
    const { useSession } = await import("@genesy/api-client/auth")
    const { useUsersMe } = await import("@genesy/api-client/endpoints/users")
    const { useRights } = await import("./can")
    vi.mocked(useSession).mockReturnValue("w1")
    vi.mocked(useUsersMe).mockReturnValue({
      isSuccess: true,
      data: {
        data: {
          user: { permissions: [{ workspace: "w1", permissions: [right] }] },
        },
      },
    } as ReturnType<typeof useUsersMe>)
    expect(useRights().may("payment.read")).toBe(right !== "product.list")
  }
)
