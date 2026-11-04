import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import { VOUCHER, expired, held, lookup, redeemed, timeline } from "./redeem.api"

/** Opens /officer/redeem signed in as a field officer; returns how many redeems reached the API. */
async function open(page: Page, lookupReply: MockReply, locale: "en" | "sw" = "en") {
  await seedSession(page)
  // The first-visit guided tour would take the officer to /officer; this officer has seen it.
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["officer:u1"])))
  await mockApi(page, {
    "GET /farm/me": me({ locale }),
    "GET /workspace/w1/farm/villages": villages,
    "POST /workspace/w1/farm/vouchers/lookup": lookupReply,
    [`GET /workspace/w1/farm/vouchers/${VOUCHER}/timeline`]: timeline,
  })
  // Registered after mockApi, so it answers first: counts redeems and answers slowly,
  // like a village connection, so a second tap lands while the first is in flight.
  const redeems = { count: 0 }
  await page.route("**/v1/workspace/w1/farm/vouchers/redeem", async (route) => {
    redeems.count++
    await new Promise((resolve) => setTimeout(resolve, 400))
    await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(redeemed) })
  })
  await page.goto("/officer/redeem")
  return redeems
}

async function lookUp(page: Page) {
  const sent = page.waitForRequest("**/v1/workspace/w1/farm/vouchers/lookup")
  await page.getByTestId("redeem-code").fill("k7qxm-2pa9d")
  await page.getByTestId("redeem-lookup").click()
  return (await sent).postDataJSON()
}

test("an officer looks up a voucher, checks the ID and the name, and hands it over", async ({ page }) => {
  const redeems = await open(page, lookup())

  expect(await lookUp(page)).toEqual({ code: "K7QXM2PA9D" })
  await expect(page.getByTestId("redeem-amount")).toHaveText("TZS 5,000.00")
  await expect(page.getByTestId("redeem-household")).toHaveText("Kaya ya Mwakalinga")

  await page.getByTestId("redeem-id-type").click()
  await page.getByRole("option", { name: "Voter card" }).click()
  await page.getByTestId("redeem-name-confirm").check()
  const sent = page.waitForRequest("**/v1/workspace/w1/farm/vouchers/redeem")
  await page.getByTestId("redeem-confirm").click()
  expect((await sent).postDataJSON()).toEqual({ code: "K7QXM2PA9D", id_type: "voter", name_confirmed: true })

  await expect(page.getByTestId("redeem-done")).toContainText("TZS 5,000.00 handed over.")
  await expect(page.getByTestId("audit-event-redeemed")).toContainText("ID checked: Voter card")
  expect(redeems.count).toBe(1)
  await expectNoHorizontalScroll(page)
})

test("a voucher the API blocks shows its reason and no hand-over", async ({ page }) => {
  const redeems = await open(page, held)
  await lookUp(page)

  await expect(page.getByTestId("redeem-blocked")).toContainText(
    "This voucher is held for an audit: ops or admin must redeem it in person"
  )
  await expect(page.getByTestId("redeem-confirm")).toHaveCount(0)
  expect(redeems.count).toBe(0)
  await expectNoHorizontalScroll(page)
})

test("an expired voucher reads Expired, says when, and offers no hand-over", async ({ page }) => {
  const redeems = await open(page, expired)
  await lookUp(page)
  await expect(page.getByTestId("redeem-preview")).toContainText("Expired")
  await expect(page.getByTestId("redeem-blocked")).toContainText("You cannot redeem this voucher")
  await expect(page.getByTestId("redeem-blocked")).toContainText(/This voucher expired on 21 Oct\w* 2026\./)
  await expect(page.getByTestId("redeem-confirm")).toHaveCount(0)
  expect(redeems.count).toBe(0)
  await expectNoHorizontalScroll(page)
})

test("a Swahili officer reads the expiry in Swahili", async ({ page }) => {
  await open(page, expired, "sw")
  await lookUp(page)
  await expect(page.getByTestId("redeem-preview")).toContainText("Imeisha muda wake")
  await expect(page.getByTestId("redeem-blocked")).toContainText("Huwezi kukabidhi vocha hii")
  await expect(page.getByTestId("redeem-blocked")).toContainText("Vocha hii iliisha muda wake tarehe 21")
  await expect(page.getByTestId("redeem-confirm")).toHaveCount(0)
})

test("a double tap on confirm pays once", async ({ page }) => {
  const redeems = await open(page, lookup())
  await lookUp(page)
  await page.getByTestId("redeem-id-type").click()
  await page.getByRole("option", { name: "NIDA card" }).click()
  await page.getByTestId("redeem-name-confirm").check()

  await page.getByTestId("redeem-confirm").dblclick()

  await expect(page.getByTestId("redeem-done")).toBeVisible()
  expect(redeems.count).toBe(1)
})
