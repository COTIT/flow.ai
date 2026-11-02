import { expect, paged, test, type FakeApi } from "./fake-api"

function transfersApi(api: FakeApi) {
  const transfers = [
    {
      id: "tr-1",
      link_id: "link-1",
      direction: "incoming",
      partner: { id: "ws-ruaha", name: "RUAHA Project" },
      from_lot: { code: "ILU-H-2026-01", public_code: "7K3M9Q2XRT" },
      quantity: "250.00",
      status: "dispatched",
      dispatched_at: "2026-10-31T08:00:00Z",
      to_lot: null as { id: string; code: string } | null,
      received_quantity: null as string | null,
      received_at: null as string | null,
    },
  ]
  const lot = {
    id: "lot-9",
    product_id: "prod-raw",
    code: "AFM-RAW-01",
    public_code: "AB12CD34EF",
    status: "released",
    produced_on: null,
    best_before: null,
    expires_on: null,
    origin: null,
    created_at: "2026-11-01T08:00:00Z",
    updated_at: "2026-11-01T08:00:00Z",
  }
  api
    .on("GET", "/workspace/:ws/supply-transfers", () => paged(transfers))
    .on("GET", "/workspace/:ws/products", () =>
      paged([{ id: "prod-raw", name: "Raw honey (kg)" }])
    )
    .on("GET", "/workspace/:ws/warehouses", () =>
      paged([{ id: "wh-1", name: "Dar es Salaam" }])
    )
    .on("POST", "/workspace/:ws/supply-transfers/:id/receive", ({ body }) => {
      const b = body as { code: string; quantity: string }
      if (b.quantity === "300")
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              quantity: [
                "The quantity may not be more than the 250.00 dispatched.",
              ],
            },
          },
        }
      Object.assign(transfers[0]!, {
        status: "received",
        to_lot: { id: lot.id, code: b.code },
        received_quantity: b.quantity,
        received_at: "2026-11-01T08:00:00Z",
      })
      return { data: transfers[0] }
    })
    // The new lot's page, opened after receiving.
    .on("GET", "/workspace/:ws/lots/:id", () => ({ data: lot }))
    .on("GET", "/workspace/:ws/lots/:id/events", () => paged([]))
}

test("receive: cancel sends nothing; the body is exact; opens the new batch", async ({
  page,
  api,
}) => {
  transfersApi(api)
  await page.goto("/supply/transfers")
  await expect(page.getByText("On its way")).toBeVisible()

  await page
    .getByRole("button", { name: "Receive batch ILU-H-2026-01" })
    .click()
  const dialog = page.getByRole("dialog")
  await expect(dialog).toContainText("can be done only once")
  await dialog.getByRole("button", { name: "Cancel" }).click()
  await expect(dialog).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/supply-transfers/:id/receive")
  ).toHaveLength(0)

  await page
    .getByRole("button", { name: "Receive batch ILU-H-2026-01" })
    .click()
  await expect(dialog.getByLabel("Quantity")).toHaveValue("250.00")
  await dialog.getByLabel("New batch code").fill("AFM-RAW-01")
  await dialog.getByLabel("Quantity").fill("248.50")
  await dialog.getByRole("button", { name: "Receive" }).click()
  await expect(page).toHaveURL(/\/lots\/lot-9\?done=received$/)
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Transfer received into this new batch." })
  ).toBeVisible()
  await expect(page.getByRole("heading", { name: "AFM-RAW-01" })).toBeVisible()
  expect(
    api.callsTo("POST", "/workspace/:ws/supply-transfers/:id/receive")[0]?.body
  ).toEqual({
    product_id: "prod-raw",
    code: "AFM-RAW-01",
    warehouse_id: "wh-1",
    quantity: "248.50",
  })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/supply-transfers/tr-1/receive", status: 422 },
      ],
    },
  })

  test("receive: more than dispatched shows the API's 422 at the quantity", async ({
    page,
    api,
  }) => {
    transfersApi(api)
    await page.goto("/supply/transfers")
    await page
      .getByRole("button", { name: "Receive batch ILU-H-2026-01" })
      .click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("New batch code").fill("AFM-RAW-01")
    await dialog.getByLabel("Quantity").fill("300")
    await dialog.getByRole("button", { name: "Receive" }).click()
    await expect(
      dialog.getByText(
        "The quantity may not be more than the 250.00 dispatched."
      )
    ).toBeVisible()
    await expect(dialog.getByLabel("Quantity")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
  })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/supply-transfers/tr-1/receive", status: 409 },
      ],
    },
  })

  test("receive: already received shows the API's 409", async ({
    page,
    api,
  }) => {
    transfersApi(api)
    api.on("POST", "/workspace/:ws/supply-transfers/:id/receive", () => ({
      status: 409,
      body: { message: "This transfer was already received." },
    }))
    await page.goto("/supply/transfers")
    await page
      .getByRole("button", { name: "Receive batch ILU-H-2026-01" })
      .click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("New batch code").fill("AFM-RAW-01")
    await dialog.getByRole("button", { name: "Receive" }).click()
    await expect(
      dialog
        .getByRole("alert")
        .filter({ hasText: "This transfer was already received." })
    ).toBeVisible()
  })
})
