import type { MockReply } from "../../src/api-mock"

/** Office API replies the specs share; override one key per test. */
export const officeApi = (
  products: unknown[] = [
    {
      id: "p1",
      name: "Ilundo Wild Forest Honey",
      product_nr: "P-001",
      reference: "ILU",
      price_incl: "14.99",
      status: "active",
    },
    {
      id: "p2",
      name: "Beeswax Block",
      product_nr: "P-002",
      reference: null,
      price_incl: "9.00",
      status: "active",
    },
  ]
): Record<string, MockReply> => ({
  "GET /me/workspaces": {
    body: { data: [{ id: "w1", workspace_name: "Ilundo Honey Co." }] },
  },
  "GET /workspace/w1/products": {
    body: {
      data: products,
      meta: {
        current_page: 1,
        from: 1,
        to: products.length,
        last_page: 1,
        per_page: 50,
        total: products.length,
      },
    },
  },
})
