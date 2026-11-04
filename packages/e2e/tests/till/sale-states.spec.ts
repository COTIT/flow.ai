import { expectTillLook } from "./look"
import {
  expect,
  honey,
  quoteReply,
  seedTill,
  serveCatalog,
  test,
} from "./fake-till"

const status = (page: import("@playwright/test").Page, text: string) =>
  page.getByRole("status").filter({ hasText: text })

test("an empty cart and a search with no match say so", async ({ page }) => {
  await seedTill(page)
  await page.goto("/sale")
  // Empty states are not live regions: matched by their words.
  await expect(page.getByText("The basket is empty.")).toBeVisible()
  await page.getByLabel("Scan a barcode or type a name").fill("zzz")
  await expect(page.getByText("No products match “zzz”.")).toBeVisible()
  await expectTillLook(page)
})

test("a register with nothing to sell says who to ask", async ({
  page,
  api,
}) => {
  serveCatalog(api, [])
  await seedTill(page)
  await page.goto("/sale")
  await expect(
    page.getByText("This register has no products to sell yet. Ask the office.")
  ).toBeVisible()
  await expectTillLook(page)
})

test("no open shift is shown before charging, with the way to the drawer", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/till/shift", () => ({ data: null }))
  await seedTill(page)
  await page.goto("/sale")
  const warning = status(page, "No shift is open on this register.")
  await expect(warning).toBeVisible()
  await expectTillLook(page)
  await warning.getByRole("link", { name: "Open the drawer" }).click()
  await expect(page).toHaveURL("/drawer")
})

test("a quote with prices that differ from the catalog warns the cashier", async ({
  page,
  api,
}) => {
  api.on("POST", "/workspace/:ws/till/sales/quote", ({ body }) => {
    const reply = quoteReply(
      body as Parameters<typeof quoteReply>[0],
      1250,
      1250
    )
    return {
      data: { ...reply.data, mismatched_lines: 1, price_mismatch: -100 },
    }
  })
  await seedTill(page)
  await page.goto("/sale")
  await page.getByRole("button", { name: /Ilundo honey 500 g/ }).click()
  await expect(
    page
      .getByRole("complementary", { name: "Basket" })
      .getByRole("status")
      .filter({ hasText: "Some prices differ from the current catalog." })
  ).toBeVisible()
})

test("a booked sale says so; a price mismatch on it is flagged", async ({
  page,
  api,
}) => {
  api.on("POST", "/workspace/:ws/till/sales", ({ body }) => {
    const b = body as { client_ref: string }
    return {
      status: 201,
      body: {
        data: {
          order_id: "o-1",
          number: "1043",
          client_ref: b.client_ref,
          register_id: "reg-1",
          till_user_id: "tu-1",
          method: "card",
          total: 1250,
          server_total: 1350,
          price_mismatch: -100,
          mismatched_lines: 1,
          due: 1250,
          rounding: 0,
          tendered: null,
          change: null,
          card_reference: null,
          sold_at: "2026-11-01T09:12:00Z",
          lines: [
            {
              title: "Ilundo honey 500 g",
              product_id: "p-honey",
              product_variant_id: null,
              quantity: "1.00",
              unit_price: 1147,
              line_excl: 1147,
              line_vat: 103,
              line_total: 1250,
            },
          ],
        },
      },
    }
  })
  await seedTill(page)
  await page.goto("/sale")
  await page.getByLabel("Scan a barcode or type a name").fill(honey.name)
  await page.getByRole("button", { name: /Ilundo honey 500 g/ }).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  await page.getByRole("button", { name: "Card" }).click()
  await page.getByRole("button", { name: /^Book card sale/ }).click()
  await expect(status(page, "Sale 1043 booked")).toBeVisible()
  await expect(
    status(page, "This sale was charged at prices that differ")
  ).toBeVisible()
  await expectTillLook(page)
})
