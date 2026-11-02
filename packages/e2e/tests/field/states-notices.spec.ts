import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { VOUCHER, held, timeline } from "./redeem.api"
import { ilundo, villageList } from "./villages.api"

/**
 * States plan step 2: field draws its states with the shared components. Zero rows is the
 * shared EmptyState (no alarm), a failed load is the error Notice (role=alert) with a retry
 * that works, and a held voucher is a warning Notice (role=status), in the officer's language.
 */
const VILLAGES = "/workspace/w1/farm/villages"
const LIST = `/v1${VILLAGES}?page=1&page_size=100`

/** Signed in as ops on /ops/villages; `routes` stays live, so a test can change an answer. */
async function openVillages(page: Page, list: MockReply) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    [`GET ${VILLAGES}`]: list,
  }
  await mockApi(page, routes)
  await page.goto("/ops/villages")
  return routes
}

test("an empty list is the shared EmptyState, not an error", async ({ page }) => {
  await openVillages(page, villageList())

  const empty = page.getByTestId("empty-state")
  await expect(empty.getByText("No villages", { exact: true })).toBeVisible()
  await expect(empty).toContainText("No villages are visible for this project.")
  await expect(empty.getByRole("alert")).toHaveCount(0)
  await expect(empty.getByRole("status")).toHaveCount(0)
  await expect(page.getByTestId("error-state")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test.describe("a list that fails to load", () => {
  test.use({ allow: { responses: [{ url: LIST, status: 500 }] } })

  test("is the error Notice, announced as an alert, and its retry loads the rows", async ({ page }) => {
    const routes = await openVillages(page, { status: 500, body: { message: "Server Error" } })

    const alert = page.getByTestId("error-state").getByRole("alert")
    await expect(alert).toBeVisible()
    await expect(alert).toContainText("Something went wrong")
    await expect(page.getByTestId("empty-state")).toHaveCount(0)
    await expectNoHorizontalScroll(page)

    routes[`GET ${VILLAGES}`] = villageList(ilundo)
    await alert.getByTestId("error-retry").click()
    await expect(page.getByTestId("village-row")).toHaveCount(1)
    await expect(page.getByTestId("error-state")).toHaveCount(0)
  })
})

/** /officer/redeem with a held voucher looked up, in the officer's language. */
async function lookUpHeld(page: Page, locale: "en" | "sw") {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["officer:u1"])))
  await mockApi(page, {
    "GET /farm/me": me({ locale }),
    "GET /workspace/w1/farm/villages": villages,
    "POST /workspace/w1/farm/vouchers/lookup": held,
    [`GET /workspace/w1/farm/vouchers/${VOUCHER}/timeline`]: timeline,
  })
  await page.goto("/officer/redeem")
  await page.getByTestId("redeem-code").fill("k7qxm-2pa9d")
  await page.getByTestId("redeem-lookup").click()
  return page.getByTestId("redeem-blocked").getByRole("status")
}

test("a held voucher is a warning Notice, not an error", async ({ page }) => {
  const warning = await lookUpHeld(page, "en")

  await expect(warning).toBeVisible()
  await expect(warning).toHaveAttribute("data-variant", "warning")
  await expect(warning).toContainText("You cannot redeem this voucher")
  await expect(warning).toContainText("This voucher is held for an audit: ops or admin must redeem it in person")
  await expect(page.getByRole("alert")).toHaveCount(0)
  await expect(page.getByTestId("redeem-confirm")).toHaveCount(0)
  await expectNoHorizontalScroll(page)
})

test("the warning is in Swahili when the officer reads Swahili", async ({ page }) => {
  const warning = await lookUpHeld(page, "sw")

  await expect(warning).toContainText("Huwezi kukabidhi vocha hii")
  await expect(warning).not.toContainText("You cannot redeem this voucher")
  await expectNoHorizontalScroll(page)
})
