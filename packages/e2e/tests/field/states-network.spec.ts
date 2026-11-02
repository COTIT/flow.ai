import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role } from "./api"
import { ilundo, mlowa, villageList } from "./villages.api"

/**
 * States plan step 1 (gaps 1-3): a request that gets no answer says "No network" with a retry —
 * never raw "Network Error", an endless "Loading…" or an empty list — and "Try again" after a
 * failed save re-sends that save unchanged, so the API replays it instead of refusing it.
 */
const VILLAGES = "/workspace/w1/farm/villages"
const LIST = `/v1${VILLAGES}?page=1&page_size=100`
const NO_NETWORK = "No network. Check your connection and try again."
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/

/** What is cut: everything but the session, only writes, or nothing. */
type Cut = "all" | "writes" | null

/** Signed in as ops on /ops/villages. `net.cut` drops calls; `net.posts` is every write the page sent. */
async function open(page: Page, cut: Cut = null) {
  await seedSession(page)
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["ops:u1"])))
  const routes: Record<string, MockReply> = {
    "GET /farm/me": me({ name: "Omari Ops", memberships: [role("ops", null)] }),
    [`GET ${VILLAGES}`]: villageList(ilundo),
    [`POST ${VILLAGES}`]: { status: 201, body: { data: mlowa } },
  }
  await mockApi(page, routes)
  const net = { cut, posts: [] as unknown[] }
  // The page's own record of what it sent, whether or not anything answered.
  page.on("request", (request) => {
    if (request.method() === "POST" && request.url().endsWith(`/v1${VILLAGES}`)) net.posts.push(request.postDataJSON())
  })
  await page.route("**/v1/**", async (route) => {
    const request = route.request()
    const path = new URL(request.url()).pathname
    const cutNow =
      (net.cut === "all" && path !== "/v1/farm/me") || (net.cut === "writes" && request.method() !== "GET")
    if (cutNow) return route.abort("internetdisconnected")
    if (request.method() === "POST") routes[`GET ${VILLAGES}`] = villageList(ilundo, mlowa)
    await route.fallback()
  })
  await page.goto("/ops/villages")
  return net
}

async function fillVillage(page: Page) {
  await page.getByTestId("village-create-open").click()
  await page.getByTestId("village-name").fill("Mlowa")
  await page.getByTestId("village-code").fill("MLW")
  await page.getByTestId("village-capacity").fill("250")
}

test.describe("a list load with no network", () => {
  test.use({ allow: { failures: [LIST] } })

  test("says No network with a retry, not an empty list; back online, retry shows the rows", async ({ page }) => {
    const net = await open(page, "all")

    const error = page.getByTestId("error-state")
    await expect(error).toContainText(NO_NETWORK)
    await expect(error).not.toContainText("Network Error")
    await expect(page.getByTestId("empty-state")).toHaveCount(0)
    await expect(page.getByTestId("village-row")).toHaveCount(0)
    await expectNoHorizontalScroll(page)

    net.cut = null
    await page.getByTestId("error-retry").click()
    await expect(page.getByTestId("village-row")).toHaveCount(1)
    await expect(page.getByTestId("error-state")).toHaveCount(0)
  })
})

test.describe("a save with no network", () => {
  test.use({ allow: { failures: [`/v1${VILLAGES}`] } })

  test("fails fast into No network; Try again sends one more request, same id, same body", async ({ page, context }) => {
    const net = await open(page)
    await expect(page.getByTestId("village-row")).toHaveCount(1)
    await fillVillage(page)

    // The browser says offline: the save is still sent and fails, instead of pausing on "Saving…".
    await context.setOffline(true)
    net.cut = "all"
    await page.getByTestId("village-create-submit").click()
    await expect(page.getByTestId("village-create-error")).toContainText(NO_NETWORK)
    expect(net.posts).toHaveLength(1)

    await context.setOffline(false)
    net.cut = null
    await page.getByTestId("village-create-error").getByTestId("error-retry").click()

    await expect(page.getByTestId("village-row")).toHaveCount(2)
    expect(net.posts).toHaveLength(2)
    expect((net.posts[0] as { id: string }).id).toMatch(UUID)
    expect(net.posts[1]).toEqual(net.posts[0])
    await expectNoHorizontalScroll(page)
  })
})

test.describe("a form filled with no network", () => {
  test.use({ allow: { failures: [`/v1${VILLAGES}`] } })

  test("keeps its draft through a reload", async ({ page, context }) => {
    const net = await open(page)
    await expect(page.getByTestId("village-row")).toHaveCount(1)

    await context.setOffline(true)
    net.cut = "all"
    await fillVillage(page)
    await page.getByTestId("village-create-submit").click()
    await expect(page.getByTestId("village-create-error")).toContainText(NO_NETWORK)

    // The page has to load again, so the browser comes back; the save stays cut.
    await context.setOffline(false)
    net.cut = "writes"
    await page.reload()
    await page.getByTestId("village-create-open").click()
    await expect(page.getByTestId("village-name")).toHaveValue("Mlowa")
    await expect(page.getByTestId("village-code")).toHaveValue("MLW")
    await expect(page.getByTestId("village-capacity")).toHaveValue("250")
    await expectNoHorizontalScroll(page)
  })
})
