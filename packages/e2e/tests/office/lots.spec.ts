import { expect, paged, test, type FakeApi } from "./fake-api"

const product = { id: "prod-honey", name: "AFM Honey 500g" }

function lotsApi(api: FakeApi) {
  const lots = [
    {
      id: "lot-1",
      product_id: product.id,
      code: "ILU-2026-01",
      public_code: "7K3M9Q2XRT",
      status: "released",
      produced_on: "2026-09-01",
      best_before: "2028-09-01",
      expires_on: null,
      origin: "Ilundo, Tanzania",
      created_at: "2026-09-02T08:00:00Z",
      updated_at: "2026-09-02T08:00:00Z",
    },
  ]
  const events = [
    {
      id: "ev-1",
      kind: "created",
      from_status: null as string | null,
      to_status: "released",
      user_id: null,
      meta: null,
      created_at: "2026-09-02T08:00:00Z",
    },
  ]
  api
    .on("GET", "/workspace/:ws/products", () => paged([product]))
    .on("GET", "/workspace/:ws/lots", ({ query }) =>
      paged(
        lots.filter(
          (l) => !query.get("status") || l.status === query.get("status")
        )
      )
    )
    .on("POST", "/workspace/:ws/lots", ({ body }) => {
      const b = body as Record<string, string>
      if (lots.some((l) => l.code.toLowerCase() === b.code?.toLowerCase()))
        return {
          status: 409,
          body: { message: "A lot with this code already exists." },
        }
      const lot = {
        ...lots[0]!,
        ...b,
        id: `lot-${lots.length + 1}`,
        public_code: "AB12CD34EF",
      }
      lots.push(lot)
      return { status: 201, body: { data: lot } }
    })
    .on("GET", "/workspace/:ws/lots/:lot", ({ params }) => ({
      data: lots.find((l) => l.id === params[1]),
    }))
    .on("PATCH", "/workspace/:ws/lots/:lot", ({ params, body }) => {
      const b = body as Record<string, string>
      if (b.best_before && b.produced_on && b.best_before < b.produced_on)
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              best_before: ["Best before must be on or after produced on."],
            },
          },
        }
      const lot = lots.find((l) => l.id === params[1])!
      if (b.status && b.status !== lot.status)
        events.unshift({
          ...events[0]!,
          id: `ev-${events.length + 1}`,
          kind: "status_changed",
          from_status: lot.status,
          to_status: b.status,
        })
      Object.assign(lot, b, { updated_at: new Date().toISOString() })
      return { data: lot }
    })
    .on("GET", "/workspace/:ws/lots/:lot/events", () => paged(events))
  return lots
}

test("create a batch: it opens and shows in the list", async ({
  page,
  api,
}) => {
  lotsApi(api)
  await page.goto("/lots")
  await expect(page.getByRole("link", { name: "ILU-2026-01" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "AFM Honey 500g" })).toBeVisible()

  await page.getByRole("button", { name: "New batch" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Batch code").fill("ILU-2026-02")
  await dialog.getByLabel("Produced on").fill("2026-10-01")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page).toHaveURL(/\/lots\/lot-2$/)
  await expect(page.getByRole("heading", { name: "ILU-2026-02" })).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/lots")[0]?.body).toMatchObject({
    product_id: product.id,
    code: "ILU-2026-02",
    produced_on: "2026-10-01",
    best_before: null,
  })
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/lots", status: 409 }] },
  })

  test("duplicate code: the API's 409 message is shown", async ({
    page,
    api,
  }) => {
    lotsApi(api)
    await page.goto("/lots")
    await page.getByRole("button", { name: "New batch" }).click()
    await page.getByRole("dialog").getByLabel("Batch code").fill("ilu-2026-01")
    await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "A lot with this code already exists." })
    ).toBeVisible()
  })
})

test("status filter goes to the API", async ({ page, api }) => {
  lotsApi(api)
  await page.goto("/lots")
  await page.getByRole("combobox", { name: "Status" }).selectOption("recalled")
  await expect(page.getByText("No batches yet.")).toBeVisible()
  expect(api.calls.at(-1)?.path).toMatch(/\/lots$/)
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/lots/lot-1", status: 422 }] },
  })

  test("edit: 422 field error shows next to the field", async ({
    page,
    api,
  }) => {
    lotsApi(api)
    await page.goto("/lots/lot-1")
    await page.getByLabel("Best before").fill("2026-01-01")
    await page.getByRole("button", { name: "Save" }).click()
    await expect(
      page.getByText("Best before must be on or after produced on.")
    ).toBeVisible()
    await expect(page.getByLabel("Best before")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
  })
})

test("recall asks first; cancelling sends nothing", async ({ page, api }) => {
  lotsApi(api)
  await page.goto("/lots/lot-1")
  await page.getByLabel("Status").selectOption("recalled")
  await page.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("alertdialog")).toContainText("ILU-2026-01")
  await page.getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("PATCH", "/workspace/:ws/lots/:lot")).toHaveLength(0)
})

test("recall confirmed: status saved and history shows the change", async ({
  page,
  api,
}) => {
  lotsApi(api)
  await page.goto("/lots/lot-1")
  await page.getByLabel("Status").selectOption("recalled")
  await page.getByRole("button", { name: "Save" }).click()
  await page.getByRole("button", { name: "Recall batch" }).click()
  await expect(page.getByText("Released → Recalled")).toBeVisible()
  expect(
    api.callsTo("PATCH", "/workspace/:ws/lots/:lot")[0]?.body
  ).toMatchObject({
    status: "recalled",
  })
})
