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
  // A member who may read products but create nothing: read-only screens, no create buttons.
  "GET /me": {
    body: {
      data: {
        user: {
          id: "u1",
          name: "Romeo Tester",
          initials: "RT",
          permissions: [{ workspace: "w1", permissions: ["product.list"] }],
        },
      },
    },
  },
  // The overview's lists (login lands there) and the menu's counts (open orders, pending links); empty.
  ...Object.fromEntries(
    [
      "orders",
      "supply-links",
      "supply-transfers",
      "lots",
      "products/stocks/negative",
      "till/sales",
      "till/shifts",
      "till/registers",
      "currencies",
    ].map((path) => [
      `GET /workspace/w1/${path}`,
      { body: { data: [], meta: { current_page: 1, from: null, to: null, last_page: 1, per_page: 1, total: 0 } } },
    ])
  ),
  "GET /workspace/w1/sales-summary": {
    body: { data: { from: "", to: "", source: null, currencies: [] } },
  },
  "GET /me/workspaces": {
    body: { data: [{ id: "w1", workspace_name: "ilundo-honey", display_name: "Ilundo Honey Co." }] },
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
