import { expect, paged, test, type FakeApi } from "./fake-api"

const usd = { id: "cur-usd", asset_type: "USD", decimals: 2, name: "US dollar" }

function tillApi(api: FakeApi) {
  const registers = [
    {
      id: "reg-1",
      name: "Shop till 1",
      warehouse_id: "wh-shop",
      currency_id: usd.id,
      key_prefix: "gtk_ab12",
      paired_at: null as string | null,
      created_at: "2026-10-01T08:00:00Z",
    },
  ]
  let codes = 0
  const users = [
    {
      id: "tu-1",
      name: "Amina",
      user_id: null,
      active: true,
      locked: true,
      created_at: "2026-10-01T08:00:00Z",
    },
  ]
  const salesQueries: string[] = []
  api
    .on("GET", "/workspace/:ws/currencies", () => paged([usd]))
    .on("GET", "/workspace/:ws/warehouses", () =>
      paged([{ id: "wh-shop", name: "Iringa shop" }])
    )
    .on("GET", "/workspace/:ws/till/registers", () => paged(registers))
    .on("POST", "/workspace/:ws/till/registers", ({ body }) => {
      const reg = {
        ...registers[0]!,
        ...(body as object),
        id: "reg-2",
        key_prefix: "gtk_cd34",
      }
      registers.push(reg)
      return {
        status: 201,
        body: { data: { ...reg, key: "gtk_cd34_DEVICE_SECRET" } },
      }
    })
    .on("POST", "/workspace/:ws/till/registers/:id/pairing", () => {
      codes += 1
      return {
        status: 201,
        body: {
          data: {
            code: codes === 1 ? "K7Q4-MZPX" : "R2D2-C3P0",
            workspace: "afm",
            expires_at: "2026-11-02T10:10:00Z",
          },
        },
      }
    })
    .on("POST", "/workspace/:ws/till/registers/:id/key", ({ params }) => {
      registers.find((r) => r.id === params[1])!.key_prefix = "gtk_ef56"
      return { data: { key: "gtk_ef56_NEW_SECRET" } }
    })
    .on("GET", "/workspace/:ws/till/users", () => ({ data: users }))
    .on("POST", "/workspace/:ws/till/users", ({ body }) => {
      const b = body as { name: string }
      users.push({ ...users[0]!, id: "tu-2", name: b.name, locked: false })
      return { status: 201, body: { data: users.at(-1) } }
    })
    .on("PATCH", "/workspace/:ws/till/users/:id", ({ params, body }) => {
      const u = users.find((x) => x.id === params[1])!
      const b = body as { active?: boolean; pin?: string }
      if (b.pin) u.locked = false
      if (b.active !== undefined) u.active = b.active
      return { data: u }
    })
    .on("GET", "/workspace/:ws/till/vaults", () => ({
      data: [
        {
          id: "v-1",
          name: "Office safe",
          currency_id: usd.id,
          balance: 125000,
          created_at: "2026-10-01T08:00:00Z",
        },
      ],
    }))
    .on("POST", "/workspace/:ws/till/vaults", () => ({
      status: 201,
      body: { data: {} },
    }))
    .on("GET", "/workspace/:ws/till/shifts", () =>
      paged([
        {
          id: "sh-1",
          register_id: "reg-1",
          opened_by: "tu-1",
          opened_at: "2026-10-31T07:00:00Z",
          opening: 5000,
          expected: 15000,
          closed_by: "tu-1",
          closed_at: "2026-10-31T17:00:00Z",
          counted: 14500,
          difference: -500,
        },
        {
          id: "sh-2",
          register_id: "reg-1",
          opened_by: "tu-1",
          opened_at: "2026-11-01T07:00:00Z",
          opening: 5000,
          expected: 6200,
          closed_by: null,
          closed_at: null,
          counted: null,
          difference: null,
        },
      ])
    )
    .on("GET", "/workspace/:ws/till/sales", ({ query }) => {
      salesQueries.push(query.toString())
      const sale = {
        order_id: "ord-7",
        number: "POS-0007",
        client_ref: "c-7",
        register_id: "reg-1",
        till_user_id: "tu-1",
        method: "cash",
        total: 2400,
        server_total: 2500,
        price_mismatch: -100,
        mismatched_lines: 1,
        due: 2400,
        rounding: 0,
        tendered: 3000,
        change: 600,
        card_reference: null,
        sold_at: "2026-11-01T09:00:00Z",
      }
      return paged(
        query.get("mismatch") === "true"
          ? [sale]
          : [
              sale,
              {
                ...sale,
                order_id: "ord-8",
                number: "POS-0008",
                price_mismatch: 0,
                mismatched_lines: 0,
                server_total: 2400,
              },
            ]
      )
    })
  return { salesQueries }
}

test("register: create leads to a pairing code; no device key is ever shown", async ({
  page,
  api,
}) => {
  tillApi(api)
  await page.goto("/till")
  await expect(page).toHaveURL(/\/till\/registers$/)
  await expect(page.getByRole("cell", { name: "Iringa shop" })).toBeVisible()
  await expect(page.getByText("Not paired")).toBeVisible()

  await page.getByRole("button", { name: "New register" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill("Shop till 2")
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog.getByLabel("Pairing code")).toHaveText("K7Q4-MZPX")
  await expect(dialog.getByText("afm")).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/till/registers")[0]?.body).toEqual(
    {
      name: "Shop till 2",
      warehouse_id: "wh-shop",
      currency_id: null,
    }
  )
  await expect(page.getByText(/gtk_/)).toHaveCount(0)

  await dialog.getByRole("button", { name: "New code" }).click()
  await expect(dialog.getByLabel("Pairing code")).toHaveText("R2D2-C3P0")
  await page.keyboard.press("Escape")
  await expect(page.getByText("R2D2-C3P0")).toHaveCount(0)
})

test("register: pair a till, and disconnect asks first and shows no key", async ({
  page,
  api,
}) => {
  tillApi(api)
  await page.goto("/till/registers")
  const row = page.getByRole("row", { name: /Shop till 1/ })
  await row.getByRole("button", { name: "Pair a till" }).click()
  await expect(
    page.getByRole("dialog").getByLabel("Pairing code")
  ).toHaveText("K7Q4-MZPX")
  expect(
    api.callsTo("POST", "/workspace/:ws/till/registers/:id/pairing")
  ).toHaveLength(1)
  await page.keyboard.press("Escape")

  await row.getByRole("button", { name: "Disconnect till" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    api.callsTo("POST", "/workspace/:ws/till/registers/:id/key")
  ).toHaveLength(0)

  await row.getByRole("button", { name: "Disconnect till" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Disconnect till" })
    .click()
  await expect(
    page.getByRole("status").filter({
      hasText: "The till on Shop till 1 is disconnected.",
    })
  ).toBeVisible()
  await expect(page.getByText(/gtk_/)).toHaveCount(0)
})

test.describe("pairing without rights", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/till/registers/reg-1/pairing", status: 403 },
      ],
    },
  })

  test("shows the error and no code", async ({ page, api }) => {
    tillApi(api)
    api.on("POST", "/workspace/:ws/till/registers/:id/pairing", () => ({
      status: 403,
      body: { message: "This action is unauthorized." },
    }))
    await page.goto("/till/registers")
    await page
      .getByRole("row", { name: /Shop till 1/ })
      .getByRole("button", { name: "Pair a till" })
      .click()
    await expect(
      page.getByRole("dialog").getByRole("alert")
    ).toContainText("This action is unauthorized.")
    await expect(page.getByLabel("Pairing code")).toHaveCount(0)
  })
})

test("till users: add, reset PIN, deactivate (cancel sends nothing)", async ({
  page,
  api,
}) => {
  tillApi(api)
  await page.goto("/till/users")
  await expect(page.getByText("Locked")).toBeVisible()

  await page.getByRole("button", { name: "New till user" }).click()
  await page.getByRole("dialog").getByLabel("Name").fill("Baraka")
  await page.getByRole("dialog").getByLabel("PIN (4 to 8 digits)").fill("4821")
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
  await expect(
    page.getByRole("cell", { name: "Baraka", exact: true })
  ).toBeVisible()
  await expect(
    page.getByRole("status").filter({ hasText: "Till user added." })
  ).toBeVisible()
  expect(api.callsTo("POST", "/workspace/:ws/till/users")[0]?.body).toEqual({
    name: "Baraka",
    pin: "4821",
  })

  await page.getByRole("button", { name: "New PIN for Amina" }).click()
  await page.getByRole("dialog").getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("dialog")).toBeHidden()
  expect(api.callsTo("PATCH", "/workspace/:ws/till/users/:id")).toHaveLength(0)
  await page.getByRole("button", { name: "New PIN for Amina" }).click()
  await page.getByRole("dialog").getByLabel("PIN (4 to 8 digits)").fill("1357")
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
  await expect(page.getByText("Locked")).toHaveCount(0)
  expect(
    api.callsTo("PATCH", "/workspace/:ws/till/users/:id")[0]?.body
  ).toEqual({ pin: "1357" })

  const amina = page.getByRole("row", { name: /Amina/ })
  await amina.getByRole("button", { name: "Deactivate" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Cancel" })
    .click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("PATCH", "/workspace/:ws/till/users/:id")).toHaveLength(1)
  await amina.getByRole("button", { name: "Deactivate" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Deactivate" })
    .click()
  await expect(amina.getByText("Inactive")).toBeVisible()
  expect(
    api.callsTo("PATCH", "/workspace/:ws/till/users/:id")[1]?.body
  ).toEqual({ active: false })
})

test("vault balances and shift differences in the register's currency", async ({
  page,
  api,
}) => {
  tillApi(api)
  await page.goto("/till/vaults")
  await expect(page.getByRole("cell", { name: "$1,250.00" })).toBeVisible()
  await page.getByRole("link", { name: "Shifts" }).click()
  await expect(page.getByRole("cell", { name: "-$5.00" })).toBeVisible()
  await expect(page.getByRole("cell", { name: "$145.00" })).toBeVisible()
  await expect(page.getByText("Open", { exact: true })).toBeVisible()
})

test("till sales: the mismatch filter goes to the API", async ({
  page,
  api,
}) => {
  const { salesQueries } = tillApi(api)
  await page.goto("/till/sales")
  await expect(page.getByRole("link", { name: "POS-0008" })).toBeVisible()
  await page.getByLabel("Only sales with other prices than the catalog").check()
  await expect(page).toHaveURL(/mismatch=true/)
  await expect(page.getByRole("link", { name: "POS-0008" })).toHaveCount(0)
  await expect(
    page.getByRole("cell", { name: "-$1.00 (1 lines)" })
  ).toBeVisible()
  expect(salesQueries.at(-1)).toBe("mismatch=true")
  await expect(page.getByRole("link", { name: "POS-0007" })).toHaveAttribute(
    "href",
    "/orders/ord-7"
  )
})
