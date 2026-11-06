import { LOT } from "../../mock/fixtures.mjs"
import {
  expect,
  expectNoHorizontalScroll,
  expectNoRawValues,
  test,
} from "../../src/test"

test("a code typed on the home page opens its batch passport", async ({
  page,
}) => {
  await page.goto("/")
  // Typed as people copy it from a label: lower case, with spaces and a dash.
  await page.getByLabel("Batch code").fill("fArm ers-001")
  await page.getByRole("button", { name: "Show its passport" }).click()
  await expect(page).toHaveURL(`/b/${LOT.farmed}`, { timeout: 15_000 })
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
})

test("the trace box works from the keyboard alone", async ({ page }) => {
  await page.goto("/")
  await page.getByLabel("Batch code").focus()
  await page.keyboard.type(LOT.released)
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL(`/b/${LOT.released}`, { timeout: 15_000 })
})

test("a code that can't be a batch code is a warning, the form kept filled", async ({
  page,
}) => {
  await page.goto("/b?code=hello")
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "That doesn't look like a batch code" })
  ).toBeVisible()
  await expect(page.getByLabel("Batch code")).toHaveValue("hello")
  await expect(page.getByLabel("Batch code")).toHaveAttribute(
    "aria-invalid",
    "true"
  )
  await expectNoRawValues(page)
  await expectNoHorizontalScroll(page)

  await page.getByLabel("Batch code").fill(LOT.released)
  await page.getByRole("button", { name: "Show its passport" }).click()
  await expect(page).toHaveURL(`/b/${LOT.released}`, { timeout: 15_000 })
})

test("an empty trace page asks for the code without a warning", async ({
  page,
}) => {
  await page.goto("/b")
  await expect(page.getByLabel("Batch code")).toBeVisible()
  await expect(page.getByRole("status")).toHaveCount(0)
})

test.describe("a well-formed code with no passport", () => {
  test.use({
    allow: { responses: [{ url: `/b/${LOT.unknown}`, status: 404 }] },
  })

  test("lands on the batch not-found page", async ({ page }) => {
    await page.goto(`/b?code=${LOT.unknown.toLowerCase()}`)
    await expect(page).toHaveURL(`/b/${LOT.unknown}`)
    await expect(
      page.getByRole("heading", { level: 1, name: "We can't find this batch" })
    ).toBeVisible()
  })
})
