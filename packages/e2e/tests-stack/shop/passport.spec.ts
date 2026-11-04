import { expect, notes, test, urls } from "../../src/stack"
import { expectNoHorizontalScroll, expectNoRawValues } from "../../src/test"

// The seed's published passports (read only).
const released = notes["passport (QR target)"]!
const recalled = notes["passport (recalled)"]!
const quarantine = notes["passport (quarantine)"]!

test("a released batch opens its passport, with no warning", async ({
  page,
}) => {
  const response = await page.goto(`${urls.shop}${released}`)
  expect(response?.status()).toBe(200)
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await expect(page.getByText("Released", { exact: true })).toBeVisible()
  await expect(page.getByText(/has been recalled/)).toHaveCount(0)
  await expect(page.locator('[data-variant="warning"]')).toHaveCount(0)
  await expectNoHorizontalScroll(page)
  await expectNoRawValues(page)
})

test("a recalled batch says so first, before the title", async ({ page }) => {
  await page.goto(`${urls.shop}${recalled}`)
  const alert = page
    .getByRole("alert")
    .filter({ hasText: "This batch has been recalled" })
  await expect(alert).toBeVisible()
  await expect(page.getByText("Recalled", { exact: true })).toBeVisible()
  const before = await alert.evaluate(
    (a) =>
      !!(
        a.compareDocumentPosition(document.querySelector("h1")!) &
        Node.DOCUMENT_POSITION_FOLLOWING
      )
  )
  expect(before, "recall banner before the title").toBe(true)
  await expectNoHorizontalScroll(page)
})

test("a batch on hold shows a warning, not a recall", async ({ page }) => {
  await page.goto(`${urls.shop}${quarantine}`)
  await expect(page.getByText("On hold", { exact: true })).toBeVisible()
  await expect(page.locator('[data-variant="warning"]')).toBeVisible()
  await expect(page.getByText(/has been recalled/)).toHaveCount(0)
})

for (const [kind, path] of [
  ["recalled", recalled],
  ["on-hold", quarantine],
] as const)
  test(`the ${kind} passport shows no raw values`, async ({ page }) => {
    test.fail(
      true,
      "the demo seed's story fact 'Extracted' is an ISO date shown as is (apps/api/src/demo/catalog.rs:350, e2e-shop)"
    )
    await page.goto(`${urls.shop}${path}`)
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
    await expectNoRawValues(page)
  })

test.describe("a code nobody published", () => {
  const url = `${urls.shop}/b/ZZZZZZZZZZ`
  test.use({ allow: { responses: [{ url, status: 404 }] } })

  test("is a real 404 with a way to the shop", async ({ page }) => {
    const response = await page.goto(url)
    expect(response?.status()).toBe(404)
    await expect(
      page.getByRole("heading", { name: "We can't find this batch" })
    ).toBeVisible()
  })
})
