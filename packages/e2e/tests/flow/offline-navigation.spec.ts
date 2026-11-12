// P6-6: navigating inside Flow while offline must keep Flow on screen (SIMULATED API).
import { expect, notice, test, tunnel } from "./offline"
import { sc } from "./scenes-api"
import { dock, s } from "./shell"
import { v } from "./voice"

// The guard override is not auto like the original, so every test asks for it here.
test.beforeEach(({ api, guard }) => void [api, guard])

test("Back to home while offline keeps the scene, the draft and says why", async ({
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
  expect(page.url(), "still in Flow").toMatch(/^http.*\/scenes\/orders$/)
  await expect(dock(page)).toBeVisible()
  await expect(page.getByLabel(v.message)).toHaveValue(
    "Draft I am still typing"
  )

  // Back online the asked-for page opens by itself (J3) and the notice goes. A re-click right
  // then (before or after the auto-open) still makes exactly one navigation, never two.
  const before = await page.evaluate(() => history.length)
  await flip(false)
  await back.click({ timeout: 1_000 }).catch(() => {})
  await expect(page).toHaveURL(/\/$/)
  await expect(notice(page)).toBeHidden()
  await page.waitForTimeout(500)
  expect(await page.evaluate(() => history.length), "one navigation").toBe(
    before + 1
  )
  await expect(page.getByLabel(v.message)).toHaveValue(
    "Draft I am still typing"
  )
})

test("a Your Flow scene opened while offline keeps Flow on screen", async ({
  page,
}) => {
  const flip = await tunnel(page)
  await page.goto("/")
  const link = page
    .getByRole("region", { name: sc.yourFlow })
    .getByRole("link")
    .first()
  await expect(link).toBeVisible()
  const href = (await link.getAttribute("href"))!

  await flip(true)
  await link.click()
  // Either the scene opens from what is already loaded, or Flow stays and says why.
  await expect
    .poll(async () =>
      new URL(page.url()).pathname === href
        ? "opened"
        : (await notice(page).isVisible())
          ? "notice"
          : "waiting"
    )
    .not.toBe("waiting")
  expect(page.url(), "still in Flow").toMatch(/^http/)
  await expect(dock(page)).toBeVisible()
  test.info().annotations.push({
    type: "offline tap",
    description: new URL(page.url()).pathname === href ? "opened" : "notice",
  })

  // Back online the scene asked for opens by itself (J3); a re-click right then is one navigation.
  const before = await page.evaluate(() => history.length)
  const opened = new URL(page.url()).pathname === href
  await flip(false)
  if (!opened) await link.click({ timeout: 1_000 }).catch(() => {})
  await expect(page).toHaveURL(new RegExp(`${href}$`))
  await page.waitForTimeout(500)
  expect(await page.evaluate(() => history.length), "one navigation").toBe(
    before + (opened ? 0 : 1)
  )
})
