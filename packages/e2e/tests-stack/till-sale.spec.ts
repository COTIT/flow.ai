import { cred, expect, loginOffice, openShift, pairTill, test, urls } from "../src/stack"

type Lot = { id: string; code: string; product_id: string }
type Credit = { order_id: string; order_number: string; till_ref: string; kind: string }
type Movement = { quantity_change: string; related_id: string | null; relationships: { stock: { warehouse_id: string } } }

// AFM-CF-01: coffee from RUAHA farmers, received into the AFM store (the till's warehouse).
const FARM_LOT = "AFM-CF-01"

test("a till sale shows up in the office: the order, the stock taken and the farmer credit", async ({
  page,
  request,
}) => {
  const { owner, register } = await pairTill(page, request)
  await openShift(page)
  const lots = await owner.get<Lot[]>(owner.ws("/lots?page_size=100"))
  const lot = lots.find((l) => l.code === FARM_LOT)!
  const { name } = await owner.get<{ name: string }>(owner.ws(`/products/${lot.product_id}`))

  await page.goto(`${urls.till}/sale`)
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  await page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(name) }).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: "Cash" }).click()
  await sheet.getByRole("button", { name: "Exact" }).click()
  const booked = page.waitForRequest((r) => r.method() === "POST" && r.url().endsWith("/till/sales"))
  await sheet.getByRole("button", { name: /^Book sale/ }).click()
  const { client_ref: ref } = (await booked).postDataJSON() as { client_ref: string }
  await expect(page.getByRole("dialog", { name: "Done" })).toBeVisible()

  // The API's side, found by this sale's own till ref (other tests sell coffee too: rule 2).
  const credits = await owner.get<Credit[]>(owner.ws(`/lots/${lot.id}/farmer-credits?page_size=100`))
  const credit = credits.find((c) => c.till_ref === ref)
  expect(credit, "farmer credit for this sale").toMatchObject({ kind: "sale" })
  const movements = await owner.get<Movement[]>(owner.ws(`/products/${lot.product_id}/stock-movements?page_size=100`))
  expect(
    movements
      .filter((m) => m.related_id === credit!.order_id)
      .map((m) => [m.relationships.stock.warehouse_id, m.quantity_change]),
    "stock taken from the till's store"
  ).toEqual([[register.warehouse_id, "-1.00"]])

  // The office's side, as staff see it.
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/orders/${credit!.order_id}`)
  await expect(page.getByRole("heading", { level: 1 })).toContainText(credit!.order_number)
  await page.goto(`${urls.office}/lots/${lot.id}`)
  await expect(
    page.getByRole("region", { name: "Farmer credits" }).getByRole("listitem").filter({ hasText: credit!.order_number })
  ).toContainText("Sale")
})
