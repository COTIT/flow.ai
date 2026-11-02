import { expect, paged, test } from "./fake-api"

test.describe(() => {
  test.use({ allow: { responses: [{ url: "/no-such-page", status: 404 }] } })

  test("an unknown address shows a not-found page with a way back", async ({
    page,
  }) => {
    await page.goto("/no-such-page")
    await expect(page.getByText("This page does not exist")).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Go to products" })
    ).toHaveAttribute("href", "/products")
  })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/lots/missing", status: 404 }],
    },
  })

  test("a batch the API does not know is 'not found' with a link back, not a retry", async ({
    page,
    api,
  }) => {
    api
      .on("GET", "/workspace/:ws/lots/:id", () => ({
        status: 404,
        body: { message: "Not found." },
      }))
      .on("GET", "/workspace/:ws/lots/:id/events", () => paged([]))
    await page.goto("/lots/missing")
    // TanStack retries three times before giving up.
    await expect(page.getByText("Not found", { exact: true })).toBeVisible({
      timeout: 20_000,
    })
    await expect(
      page.getByRole("link", { name: "Back to the list" })
    ).toHaveAttribute("href", "/lots")
    await expect(page.getByRole("button", { name: "Try again" })).toHaveCount(0)
  })
})

test("an order with nothing left to do says so", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/orders/:id", () => ({
    data: {
      id: "ord-1",
      number: "SO-1001",
      status: "cancelled",
      source: "office",
      currency: "USD",
      decimals: 2,
      customer: null,
      reference: null,
      subtotal: 0,
      vat: 0,
      total: 0,
      paid_at: null,
      invoice_id: null,
      allowed_actions: [],
      lines: [],
      created_at: "2026-10-30T09:00:00Z",
    },
  }))
  await page.goto("/orders/ord-1")
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Nothing more to do on this order." })
  ).toBeVisible()
})

test("a picker cut at the first page warns how much it shows", async ({
  page,
  api,
}) => {
  const products = Array.from({ length: 100 }, (_, i) => ({
    id: `p${i}`,
    name: `Jar ${i}`,
  }))
  api
    .on("GET", "/workspace/:ws/lots", () => paged([]))
    .on("GET", "/workspace/:ws/products", () => ({
      data: products,
      meta: {
        current_page: 1,
        last_page: 2,
        per_page: 100,
        total: 150,
        from: 1,
        to: 100,
      },
    }))
  await page.goto("/lots")
  await page.getByRole("button", { name: "New batch" }).click()
  await expect(
    page
      .getByRole("dialog")
      .getByRole("status")
      .filter({ hasText: "Showing the first 100 of 150." })
  ).toBeVisible()
})

test.describe(() => {
  // React logs the caught error with a stack of build-hashed chunk names: match its start.
  test.use({
    allow: {
      consolePrefix: [
        "TypeError: Cannot read properties of null (reading 'name')",
      ],
    },
  })

  test("a screen that crashes shows the error page with a retry", async ({
    page,
    api,
  }) => {
    // A product reply without its data makes the page throw while rendering.
    api
      .on("GET", "/workspace/:ws/products/:id", () => ({ data: null }))
      .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({
        data: [],
      }))
      .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
    await page.goto("/products/p1")
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Something went wrong on this page." })
    ).toBeVisible()
    await expect(page.getByRole("button", { name: "Try again" })).toBeVisible()
  })
})
