// J3: "This opens when you're back online" is a promise. Back online, the page the person asked
// for opens by itself (once), unless they did something else meanwhile; offline lines clear.
import { expect, notice, test, tunnel } from "./offline"
import { sc } from "./scenes-api"
import { dock, s } from "./shell"
import { v } from "./voice"

test.beforeEach(({ api, guard }) => void [api, guard])

const card = (page: import("@playwright/test").Page) =>
  page.getByRole("region", { name: sc.yourFlow }).getByRole("link").first()

test("a Your Flow card tapped offline opens by itself once back online", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/")
  await expect(card(page)).toBeVisible()
  const href = (await card(page).getAttribute("href"))!
  await flip(true)
  await card(page).click()
  await expect(notice(page)).toBeVisible()
  expect(new URL(page.url()).pathname).toBe("/")

  await flip(false)
  await expect(page).toHaveURL(new RegExp(`${href}$`))
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible()
  await expect(notice(page)).toBeHidden()
  await expect(dock(page)).toBeVisible()
})

test("something else done meanwhile: back online nothing opens by surprise", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/")
  await expect(card(page)).toBeVisible()
  await flip(true)
  await card(page).click()
  await expect(notice(page)).toBeVisible()
  // The person moves on: types in the message box.
  await page.getByLabel(v.message).fill("Something else")

  await flip(false)
  await expect(notice(page)).toBeHidden()
  await page.waitForTimeout(1_000)
  expect(new URL(page.url()).pathname, "no surprise navigation").toBe("/")
  await expect(page.getByLabel(v.message)).toHaveValue("Something else")
})

test("Back to home tapped offline in a scene goes home once back online, draft kept", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/scenes/orders")
  const back = page.getByRole("button", { name: s.backHome })
  await expect(back).toBeVisible()
  await page.getByLabel(v.message).fill("Draft I am still typing")
  await flip(true)
  await back.click()
  await expect(notice(page)).toBeVisible()

  await flip(false)
  await expect(page).toHaveURL(/\/$/)
  await expect(notice(page)).toBeHidden()
  await expect(page.getByLabel(v.message)).toHaveValue(
    "Draft I am still typing"
  )
})

test("the voice offline alert goes once the browser is back online", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/")
  await expect(
    page.getByRole("button", { name: v.textInstead, exact: true })
  ).toBeVisible()
  await flip(true)
  await page.getByRole("button", { name: v.textInstead, exact: true }).click()
  const offlineAlert = dock(page)
    .getByRole("alert")
    .filter({ hasText: v.offline })
  await expect(offlineAlert).toBeVisible()

  await flip(false)
  await expect(offlineAlert).toBeHidden({ timeout: 3_000 })
})
