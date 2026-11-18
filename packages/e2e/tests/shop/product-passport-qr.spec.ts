import { LOT, PRODUCT, SHOP_CONNECTIONS } from "../../mock/fixtures.mjs"
import { expect, test } from "../../src/test"

const shop = `/s/${SHOP_CONNECTIONS[0]}`

test("a scoped product passport QR opens its published batch summary", async ({
  page,
}) => {
  await page.goto(`${shop}/products/${PRODUCT.honey}#jar-passport`)
  const target = page.locator("#jar-passport")
  await expect(target).toHaveText("Where it came from")
  await expect(target).toBeInViewport()
  const passport = page.getByRole("region", { name: "Where it came from" })
  await expect(
    passport.getByRole("link", { name: "Open the passport" })
  ).toHaveAttribute("href", `${shop}/b/${LOT.farmed}`)
})

test("a scoped product passport QR offers batch lookup when the batch is unpublished", async ({
  page,
}) => {
  await page.goto(`${shop}/products/${PRODUCT.wax}#jar-passport`)
  const target = page.locator("#jar-passport")
  await expect(target).toHaveText("Where it came from")
  await expect(target).toBeInViewport()
  const passport = page.getByRole("region", { name: "Where it came from" })
  await expect(passport).toContainText("No batch passport is available")
  const lookup = passport.getByRole("link", { name: "Enter a batch code" })
  await expect(lookup).toHaveAttribute("href", `${shop}/b`)
  await lookup.click()
  await expect(page).toHaveURL(`${shop}/b`)
})
