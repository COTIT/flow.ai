import { expect, paged, test } from "./fake-api"

// Each detail page, the API read that says 404, and the list its "Back to the list" link returns to.
const details: { route: string; read: string; url: string; back: string; also?: string[] }[] = [
  { route: "/products/missing", read: "/workspace/:ws/products/:id", url: "/v1/workspace/w1/products/missing", back: "/products" },
  { route: "/orders/missing", read: "/workspace/:ws/orders/:id", url: "/v1/workspace/w1/orders/missing", back: "/orders" },
  { route: "/invoices/missing", read: "/workspace/:ws/invoices/:id", url: "/v1/workspace/w1/invoices/missing", back: "/invoices" },
  {
    route: "/lots/missing/passport",
    read: "/workspace/:ws/lots/:id",
    url: "/v1/workspace/w1/lots/missing",
    back: "/lots",
    // The passport is read beside its batch, and is just as unknown.
    also: ["/v1/workspace/w1/lots/missing/passport"],
  },
]

for (const d of details)
  test.describe(() => {
    test.use({
      allow: {
        responses: [d.url, ...(d.also ?? [])].map((url) => ({ url, status: 404 })),
      },
    })

    test(`${d.route}: a record the API does not know is 'not found' with a link back, not a retry`, async ({
      page,
      api,
    }) => {
      api
        .on("GET", d.read, () => ({ status: 404, body: { message: "Not found." } }))
        // Reads some detail pages make beside the record itself.
        .on("GET", "/workspace/:ws/products/:id/stock-with-bins", () => ({ data: [] }))
        .on("GET", "/workspace/:ws/products/:id/stock-movements", () => paged([]))
        .on("GET", "/workspace/:ws/products/:id/images", () => ({ data: [] }))
        .on("GET", "/workspace/:ws/warehouses", () => paged([]))
        .on("GET", "/workspace/:ws/files", () => paged([]))
        .on("GET", "/workspace/:ws/lots/:id/passport", () => ({ status: 404, body: { message: "Not found." } }))
      await page.goto(d.route)
      // TanStack retries three times before giving up.
      await expect(page.getByText("Not found", { exact: true }), `${d.route} 404`).toBeVisible({
        timeout: 20_000,
      })
      await expect(page.getByRole("link", { name: "Back to the list" })).toHaveAttribute("href", d.back)
      await expect(page.getByRole("button", { name: "Try again" })).toHaveCount(0)
      // No action on a record that is not there.
      await expect(page.getByRole("button", { name: /Save|Publish|Complete|Make PDF/ })).toHaveCount(0)
    })
  })
