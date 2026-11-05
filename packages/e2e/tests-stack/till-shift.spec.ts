import {
  cred,
  expect,
  loginOffice,
  openShift,
  pairTill,
  test,
  urls,
} from "../src/stack"
import {
  ownProduct,
  salesOf,
  tile,
  tillMoney,
  toSale,
  type Shift,
} from "./till"

type Movement = { quantity_change: string; related_id: string | null }

test("a cash sale is an office order, takes stock, is listed in the office and counted when the drawer closes", async ({
  page,
  request,
}) => {
  const { owner, register } = await pairTill(page, request)
  await openShift(page)
  const product = await ownProduct(owner, register, "shift")
  const money = await tillMoney(page)

  await toSale(page)
  await (await tile(page, product.name)).click()
  await page.getByRole("button", { name: /^Charge/ }).click()
  const sheet = page.getByRole("dialog", { name: "Take payment" })
  await sheet.getByRole("button", { name: "Cash" }).click()
  // One unit typed: the API's quote says what is still due, and nothing is refused (the
  // guard fails the test on any 422).
  await sheet.getByRole("button", { name: "1", exact: true }).click()
  await expect(sheet.getByLabel("Still due")).toBeVisible()
  await sheet.getByRole("button", { name: "Exact" }).click()
  await expect(sheet.getByLabel("Change")).toHaveText(money(0))
  await sheet.getByRole("button", { name: /^Book sale/ }).click()
  const receipt = page.getByRole("dialog", { name: "Done" })
  await expect(
    receipt.getByRole("status").filter({ hasText: /^Sale .+ booked/ })
  ).toBeVisible()

  // The API booked exactly this sale, and the receipt shows its figures.
  const sales = await salesOf(owner, register)
  expect(sales, "sales on this register").toHaveLength(1)
  const sale = sales[0]!
  expect(sale.method).toBe("cash")
  await expect(receipt.getByText(`Sale ${sale.number} booked`)).toBeVisible()
  await expect(receipt.getByLabel("Give change")).toHaveText(money(0))
  const movements = await owner.get<Movement[]>(
    owner.ws(`/products/${product.id}/stock-movements?page_size=100`)
  )
  expect(
    movements
      .filter((m) => m.related_id === sale.order_id)
      .map((m) => m.quantity_change),
    "stock taken by the sale"
  ).toEqual(["-1.00"])

  // The open shift counts the cash in: the till's Expected is the API's.
  const [open] = await owner.get<Shift[]>(
    owner.ws(`/till/shifts?register_id=${register.id}&open=true`)
  )
  expect(open!.expected).toBe(open!.opening + sale.due)
  await receipt.getByRole("button", { name: "New sale" }).click()
  await page.getByRole("link", { name: "Drawer", exact: true }).click()
  await expect(page.getByLabel("Expected")).toHaveText(money(open!.expected))

  // While counting, the live total and difference are the API's (count-quote), not the till's.
  await page.getByLabel(`Pieces of ${money(2000)}`).fill("1")
  await expect(page.getByLabel("Counted")).toHaveText(money(2000))
  await expect(page.getByLabel("Difference")).toHaveText(money(2000 - open!.expected))
  await page.getByLabel(`Pieces of ${money(2000)}`).fill("")

  // An empty drawer at close: short by the sale, as the API works it out.
  await page.getByRole("button", { name: "Close shift" }).click()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: `Shift closed — short by ${money(open!.expected)}` })
  ).toBeVisible()
  const [closed] = await owner.get<Shift[]>(
    owner.ws(`/till/shifts?register_id=${register.id}&open=false`)
  )
  expect(closed).toMatchObject({ counted: 0, difference: -open!.expected })
  await expect(page.getByLabel("Difference")).toHaveText(
    money(closed!.difference!)
  )

  // The office: the sale in the register's till sales, leading to its order.
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/till/sales?register_id=${register.id}`)
  await page.getByRole("link", { name: sale.number }).click()
  await expect(page.getByRole("heading", { level: 1 })).toContainText(
    sale.number
  )
})
