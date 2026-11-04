import { expectNoRawValues } from "../../src/test"
import {
  apiAs,
  cred,
  expect,
  loginOffice,
  pairTill,
  stackAllow,
  test,
  unique,
  urls,
  workspaces,
} from "../../src/stack"

type Warehouse = { id: string; is_store: boolean }

test("office: a new product and its size reach the shop and the till", async ({ page, request }) => {
  // The till first: pairing signs this page into the till; the office login comes after.
  const { owner, register } = await pairTill(page, request)
  const name = `e2e product ${unique()}`

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/products`)
  await page.getByRole("button", { name: "New product" }).click()
  const create = page.getByRole("dialog")
  await create.getByLabel("Name").fill(name)
  await create.getByLabel("Price excl. VAT").fill("4.00")
  await create.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Product created." })).toBeVisible()
  await expect(page.getByRole("heading", { name })).toBeVisible()
  const id = new URL(page.url()).pathname.split("/").at(-1)!

  const sizes = page.getByRole("region", { name: "Sizes" })
  await sizes.getByRole("button", { name: "Add size" }).click()
  const size = page.getByRole("dialog")
  await size.getByLabel("Size").fill("250 g")
  await size.getByLabel("Price excl. VAT").fill("4.20")
  await size.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Size added." })).toBeVisible()
  await expect(sizes.getByText("250 g")).toBeVisible()
  await expectNoRawValues(page)

  // The office list finds it by name.
  await page.goto(`${urls.office}/products?q=${encodeURIComponent(name)}`)
  await expect(page.getByRole("link", { name })).toBeVisible()

  // Stock in the till's store and the web warehouse, so both apps can sell it.
  const warehouses = await owner.get<Warehouse[]>(owner.ws("/warehouses"))
  const depot = warehouses.find((w) => !w.is_store)!
  await owner.post(owner.ws(`/products/${id}/set-stock-levels`), {
    warehouses: [
      { warehouse_id: register.warehouse_id, stock_lvl: "20.00", reason: "e2e" },
      { warehouse_id: depot.id, stock_lvl: "20.00", reason: "e2e" },
    ],
  })

  // Shop: the product page, with the new size as a choice beside the standard one.
  await page.goto(`${urls.shop}/products/${id}`)
  await expect(page.getByRole("heading", { name })).toBeVisible()
  await expect(page.getByRole("main").getByRole("button", { name: "250 g" })).toBeVisible()
  await expectNoRawValues(page)

  // Till: a tile on the sale screen.
  await page.goto(`${urls.till}/sale`)
  await page.getByLabel("Scan a barcode or type a name").fill(name)
  await expect(
    page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(name) }).first()
  ).toBeVisible()
})

test.describe(() => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        { url: `${urls.office}/v1/workspace/${workspaces.AFM}/products`, status: 422 },
      ],
    },
  })

  test("office: a name the API refuses shows at the field and makes no product", async ({ page, request }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const tag = `e2e ${unique()}`
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/products`)
    await page.getByRole("button", { name: "New product" }).click()
    const create = page.getByRole("dialog")
    await create.getByLabel("Name").fill(`${tag} ${"x".repeat(260)}`)
    await create.getByLabel("Price excl. VAT").fill("4.00")
    await create.getByRole("button", { name: "Save" }).click()
    await expect(create.getByLabel("Name")).toHaveAttribute("aria-invalid", "true")
    await expect(create.getByLabel("Name")).toHaveAccessibleDescription(/255/)
    await expect(page).toHaveURL(`${urls.office}/products`)
    expect(await owner.get<unknown[]>(owner.ws(`/products?q=${encodeURIComponent(tag)}`))).toHaveLength(0)
  })
})

// The privacy boundary: the RUAHA admin, signed in to the office, opens an AFM product's address.
let foreign = ""
test.describe(() => {
  test.beforeAll(async ({ request }) => {
    const afm = await apiAs(request, cred("office (AFM)", "owner"))
    foreign = (
      await afm.post<{ product_id: string }>(afm.ws("/products"), { name: `e2e private ${unique()}`, price: "4.00" })
    ).product_id
  })
  // A fixture function, so the id beforeAll made is read when the test starts.
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          // The product and the reads beside it (stock, movements) are all refused with 404.
          ...["", "/stock-with-bins", "/stock-movements?page=1"].map((tail) => ({
            url: `${urls.office}/v1/workspace/${workspaces.RUAHA}/products/${foreign}${tail}`,
            status: 404,
          })),
        ],
      }),
  })

  test("office: another account's product is refused, not shown", async ({ page }) => {
    await loginOffice(page, cred("office (RUAHA)", "admin"))
    await page.goto(`${urls.office}/products/${foreign}`)
    // TanStack retries three times before giving up.
    await expect(page.getByText("Not found", { exact: true })).toBeVisible({ timeout: 20_000 })
    await expect(page.getByRole("link", { name: "Back to the list" })).toHaveAttribute("href", "/products")
    await expect(page.getByText("e2e private")).toHaveCount(0)
    await expect(page.getByRole("button", { name: "Save changes" })).toHaveCount(0)
  })
})
