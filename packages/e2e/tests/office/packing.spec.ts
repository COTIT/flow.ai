import { expect, paged, test, type FakeApi } from "./fake-api"

function packingApi(api: FakeApi) {
  const lot = {
    id: "lot-raw",
    product_id: "prod-raw",
    code: "AFM-RAW-01",
    public_code: "AB12CD34EF",
    status: "released",
    next_statuses: ["quarantine", "recalled"],
    produced_on: null,
    best_before: null,
    expires_on: null,
    origin: null,
    created_at: "2026-11-01T08:00:00Z",
    updated_at: "2026-11-01T08:00:00Z",
  }
  api
    .on("GET", "/workspace/:ws/lots/:id", () => ({ data: lot }))
    .on("GET", "/workspace/:ws/lots/:id/events", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id", () => ({
      data: { id: "prod-raw", name: "AFM raw honey" },
    }))
    .on("GET", "/workspace/:ws/lots/:id/passport", () => ({
      data: { draft: {}, published: null, published_at: null, version: 0 },
    }))
    .on("GET", "/workspace/:ws/products", () =>
      paged([{ id: "prod-jar", name: "AFM Honey 500g" }])
    )
    .on("GET", "/workspace/:ws/warehouses", () =>
      paged([{ id: "wh-1", name: "Dar es Salaam" }])
    )
    .on("POST", "/workspace/:ws/lots/:id/packings", ({ body }) => {
      const b = body as {
        code: string
        units: string
        raw_kg: string
        raw?: string
      }
      if (b.raw_kg === "999")
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: {
              raw_kg: ["The lot holds only 248.50 kg free in this warehouse."],
            },
          },
        }
      return {
        status: 201,
        body: {
          data: {
            id: "lot-jar",
            code: b.code,
            public_code: "JR12JR34JR",
            product_id: "prod-jar",
            parent_lot_id: lot.id,
            pack_raw_kg: b.raw_kg,
            pack_units: b.units,
            best_before: "2028-09-30",
            expires_on: null,
          },
        },
      }
    })
}

test("pack into jars: cancel sends nothing; body exact; result as the API gives it", async ({
  page,
  api,
}) => {
  packingApi(api)
  await page.goto("/lots/lot-raw")
  await page.getByRole("button", { name: "Pack into jars" }).click()
  const dialog = page.getByRole("dialog")
  await expect(dialog).toContainText("go into stock at once")
  await dialog.getByRole("button", { name: "Cancel" }).click()
  await expect(dialog).toBeHidden()
  expect(api.callsTo("POST", "/workspace/:ws/lots/:id/packings")).toHaveLength(
    0
  )

  await page.getByRole("button", { name: "Pack into jars" }).click()
  await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
  await dialog.getByLabel("New batch code").fill("ILU-J-2026-01")
  await dialog.getByLabel("Jars made").fill("10")
  await dialog.getByLabel("Kg used").fill("7.50")
  await dialog.getByLabel("Best before").fill("2028-09-30")
  await dialog.getByRole("button", { name: "Pack into jars" }).click()
  await expect(dialog.getByRole("status")).toHaveText(
    /ILU-J-2026-01 made: 10 jars from 7\.50 kg/
  )
  await expect(
    dialog.getByRole("link", { name: "Open batch ILU-J-2026-01" })
  ).toHaveAttribute("href", "/lots/lot-jar")
  expect(
    api.callsTo("POST", "/workspace/:ws/lots/:id/packings")[0]?.body
  ).toEqual({
    product_id: "prod-jar",
    code: "ILU-J-2026-01",
    units: "10",
    raw_kg: "7.50",
    warehouse_id: "wh-1",
    best_before: "2028-09-30",
    expires_on: null,
  })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/lots/lot-raw/packings", status: 422 },
      ],
    },
  })

  test("pack: a lot that can't be packed shows the API's 422", async ({
    page,
    api,
  }) => {
    packingApi(api)
    api.on("POST", "/workspace/:ws/lots/:id/packings", () => ({
      status: 422,
      body: {
        message:
          "Only a lot from a supply batch or a supply transfer can be packed.",
      },
    }))
    await page.goto("/lots/lot-raw")
    await page.getByRole("button", { name: "Pack into jars" }).click()
    const dialog = page.getByRole("dialog")
    // The form packs into a warehouse: wait until they are read.
    await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
    await dialog.getByLabel("New batch code").fill("X")
    await dialog.getByLabel("Jars made").fill("1")
    await dialog.getByLabel("Kg used").fill("1")
    await dialog.getByRole("button", { name: "Pack into jars" }).click()
    await expect(
      dialog.getByRole("alert").filter({
        hasText:
          "Only a lot from a supply batch or a supply transfer can be packed.",
      })
    ).toBeVisible()
  })

  test("pack: more kg than free shows at the kg field", async ({
    page,
    api,
  }) => {
    packingApi(api)
    await page.goto("/lots/lot-raw")
    await page.getByRole("button", { name: "Pack into jars" }).click()
    const dialog = page.getByRole("dialog")
    // The form packs into a warehouse: wait until they are read.
    await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
    await dialog.getByLabel("New batch code").fill("ILU-J-2026-02")
    await dialog.getByLabel("Jars made").fill("10")
    await dialog.getByLabel("Kg used").fill("999")
    await dialog.getByRole("button", { name: "Pack into jars" }).click()
    await expect(
      dialog.getByText("The lot holds only 248.50 kg free in this warehouse.")
    ).toBeVisible()
    await expect(dialog.getByLabel("Kg used")).toHaveAttribute(
      "aria-invalid",
      "true"
    )
  })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/lots/lot-raw/packings", status: 409 }],
    },
  })

  for (const [code, text] of [
    ["lot_recalled", "This batch is recalled, so it can't be packed."],
    ["lot_quarantined", "This batch is on hold, so it can't be packed. Release it first."],
  ] as const)
    test(`pack: a ${code} refusal reads as a sentence, not a code`, async ({ page, api }) => {
      packingApi(api)
      api.on("POST", "/workspace/:ws/lots/:id/packings", () => ({
        status: 409,
        body: { message: code },
      }))
      await page.goto("/lots/lot-raw")
      await page.getByRole("button", { name: "Pack into jars" }).click()
      await fillPack(page)
      const dialog = page.getByRole("dialog")
      await expect(dialog.getByRole("alert").filter({ hasText: text })).toBeVisible()
      await expect(dialog).not.toContainText(code)
    })
})

async function fillPack(page: import("@playwright/test").Page) {
  const dialog = page.getByRole("dialog")
  // The form packs into a warehouse: wait until they are read.
  await expect(dialog.getByLabel("Warehouse")).toHaveValue("wh-1")
  await dialog.getByLabel("New batch code").fill("ILU-J-2026-03")
  await dialog.getByLabel("Jars made").fill("10")
  await dialog.getByLabel("Kg used").fill("7.50")
  await dialog.getByRole("button", { name: "Pack into jars" }).click()
}

test.describe(() => {
  test.use({
    allow: {
      responses: [{ url: "/v1/workspace/w1/lots/lot-raw/packings", status: 403 }],
    },
  })

  test("/lots/[id] pack: the API's 403 shows in the dialog; no batch is made", async ({
    page,
    api,
  }) => {
    packingApi(api)
    api.on("POST", "/workspace/:ws/lots/:id/packings", () => ({
      status: 403,
      body: { message: "This action is unauthorized." },
    }))
    await page.goto("/lots/lot-raw")
    await page.getByRole("button", { name: "Pack into jars" }).click()
    await fillPack(page)
    const dialog = page.getByRole("dialog")
    await expect(
      dialog.getByRole("alert").filter({ hasText: "This action is unauthorized." })
    ).toBeVisible()
    await expect(dialog.getByRole("link", { name: /Open batch/ })).toHaveCount(0)
    expect(api.callsTo("POST", "/workspace/:ws/lots/:id/packings")).toHaveLength(1)
  })
})

test("/lots/[id] pack: retried after a network failure, the same Idempotency-Key goes out; one batch is made", async ({
  page,
  api,
}) => {
  test.fail(
    true,
    "BUG: packing sends no Idempotency-Key (apps/office/app/(app)/lots/[id]/pack-dialog.tsx:42) and POST /lots/{lot}/packings takes none, so a retry after a lost reply makes a second jar batch from the same kg"
  )
  packingApi(api)
  const keys: string[] = []
  let fail = true
  await page.route("**/v1/workspace/w1/lots/lot-raw/packings", async (route) => {
    keys.push(route.request().headers()["idempotency-key"] ?? "")
    if (fail) {
      fail = false
      return route.abort("aborted")
    }
    return route.fallback()
  })
  await page.goto("/lots/lot-raw")
  await page.getByRole("button", { name: "Pack into jars" }).click()
  await fillPack(page)
  const dialog = page.getByRole("dialog")
  await expect(
    dialog.getByRole("alert").filter({ hasText: "Something went wrong. Try again." })
  ).toBeVisible()
  await dialog.getByRole("button", { name: "Pack into jars" }).click()
  await expect(dialog.getByRole("link", { name: "Open batch ILU-J-2026-03" })).toBeVisible()
  expect(keys).toHaveLength(2)
  expect(keys[0]).toMatch(/^[0-9a-f-]{36}$/)
  expect(keys[1]).toBe(keys[0])
})
