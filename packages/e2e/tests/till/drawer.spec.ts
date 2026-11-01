import { expectNoHorizontalScroll } from "../../src/test"
import { expect, paged, seedTill, test, type FakeApi, WS } from "./fake-till"

const honey = {
  id: "p-honey",
  name: "Ilundo honey 500 g",
  brand: null,
  type: "physical",
  short_description: null,
  description: null,
  ean13: "8712345678906",
  barcode: null,
  price_gross: 1250,
  price_net: 1147,
  currency: "EUR",
  currency_decimals: 2,
  available_stock: "7.00",
  variants: [],
}

const openShift = {
  id: "sh-1",
  register_id: "reg-1",
  opened_by: "tu-1",
  opened_at: "2026-11-01T08:00:00Z",
  opening: 4515,
  expected: 6515,
  closed_by: null,
  closed_at: null,
  counted: null,
  difference: null,
}

function drawerApi(api: FakeApi, start: typeof openShift | null) {
  let shift = start
  api
    .on("GET", "/storefront/products", () => paged([honey]))
    .on("GET", "/workspace/:ws/till/shift", () => ({ data: shift }))
    .on("POST", "/workspace/:ws/till/shift/open", () => {
      shift = { ...openShift, expected: 4515 }
      return { status: 201, body: { data: shift } }
    })
    .on("POST", "/workspace/:ws/till/shift/close", () => {
      const closed = {
        ...shift!,
        closed_by: "tu-1",
        closed_at: "2026-11-01T17:00:00Z",
        counted: 6500,
        difference: -15,
      }
      shift = null
      return { data: closed }
    })
}

test("opening a shift sends the counted float and shows the API's figures", async ({
  page,
  api,
}) => {
  drawerApi(api, null)
  await seedTill(page)
  await page.goto("/sale")
  await page.getByRole("link", { name: "Drawer" }).click()
  await expect(
    page.getByText("No shift is open on this register.")
  ).toBeVisible()

  await page.getByLabel("Value of row 1").fill("20")
  await page.getByLabel("Pieces of row 1").fill("2")
  await page.getByRole("button", { name: "Add row" }).click()
  await page.getByLabel("Value of row 2").fill("0,05")
  await page.getByLabel("Pieces of row 2").fill("3")
  await expect(page.getByText("Counted: €40.15")).toBeVisible()
  await expectNoHorizontalScroll(page)
  await page.getByRole("button", { name: "Open shift" }).click()

  await expect(page.getByText("€45.15").first()).toBeVisible()
  const [open] = api.callsTo("POST", `/workspace/${WS}/till/shift/open`)
  expect(open!.body).toEqual({
    count: [
      { denomination: 2000, count: 2 },
      { denomination: 5, count: 3 },
    ],
  })
})

test("closing a shift shows the variance the API returns", async ({
  page,
  api,
}) => {
  drawerApi(api, openShift)
  await seedTill(page)
  await page.goto("/drawer")
  await expect(page.getByText("€65.15")).toBeVisible()

  await page.getByLabel("Value of row 1").fill("65")
  await page.getByLabel("Pieces of row 1").fill("1")
  await page.getByRole("button", { name: "Close shift" }).click()

  await expect(
    page.getByRole("heading", { name: "Shift closed" })
  ).toBeVisible()
  await expect(page.getByText("-€0.15")).toBeVisible()
  await expect(page.getByText("Short")).toBeVisible()
  await expect(
    page.getByText("No shift is open on this register.")
  ).toBeVisible()
})

test("a row without a valid value is refused before sending", async ({
  page,
  api,
}) => {
  drawerApi(api, null)
  await seedTill(page)
  await page.goto("/drawer")
  await page.getByLabel("Value of row 1").fill("5.005")
  await page.getByLabel("Pieces of row 1").fill("1")
  await page.getByRole("button", { name: "Open shift" }).click()
  await expect(
    page
      .getByRole("alert")
      .filter({
        hasText:
          "Each row needs a value (e.g. 5.00) and a whole number of pieces.",
      })
  ).toBeVisible()
  expect(api.callsTo("POST", `/workspace/${WS}/till/shift/open`)).toHaveLength(
    0
  )
})

test.describe("shift already open elsewhere", () => {
  test.use({
    allow: {
      responses: [{ url: `/v1/workspace/${WS}/till/shift/open`, status: 409 }],
    },
  })

  test("shows the API's message", async ({ page, api }) => {
    drawerApi(api, null)
    api.on("POST", "/workspace/:ws/till/shift/open", () => ({
      status: 409,
      body: { message: "A shift is already open on this register." },
    }))
    await seedTill(page)
    await page.goto("/drawer")
    await page.getByLabel("Value of row 1").fill("10")
    await page.getByLabel("Pieces of row 1").fill("1")
    await page.getByRole("button", { name: "Open shift" }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "A shift is already open on this register." })
    ).toBeVisible()
  })
})
