// F2: home sections over a SIMULATED API (tests/flow/scenes-api.ts). Checklist B, D; design 9 part.
import { expect, test } from "./storefront-api"
import { sc, sceneWorld } from "./scenes-api"
import { expectOrbClear, orb, orbAt } from "./shell"
import { watch } from "./voice"

// Every test signs in and gets the stand-in API, also those that never touch `api`.
test.beforeEach(({ api }) => void api)

const noticed = (page: import("@playwright/test").Page) =>
  page.getByRole("region", { name: sc.noticed.label })
const yourFlow = (page: import("@playwright/test").Page) =>
  page.getByRole("region", { name: sc.yourFlow })

test("home shows Flow noticed and Your Flow without starting any conversation", async ({
  page,
}) => {
  const sessions = watch(page, "session")
  let sockets = 0
  page.on("websocket", () => sockets++)
  await page.goto("/")
  await expect(noticed(page)).toContainText("2 priorities need your attention.")
  await expect(noticed(page)).toContainText("affects 8 open orders")
  await expect(noticed(page)).toContainText(
    "3 products below their minimum stock levels"
  )
  for (const name of [
    "Morning Pulse",
    "Stock Watch",
    "Order Focus",
    "Sales Perspective",
    "My Routines",
    "Storefront Studio",
  ])
    await expect(
      yourFlow(page).getByRole("link", { name: new RegExp(name) })
    ).toBeVisible()
  await expect(
    yourFlow(page).getByRole("link", { name: /Stock Watch/ })
  ).toContainText("3")
  await expect(
    yourFlow(page).getByRole("link", { name: /Morning Pulse/ })
  ).toContainText("€12,840.00")
  // Flow noticed has its own card: never a Your Flow preview too.
  await expect(
    yourFlow(page).getByRole("link", { name: /Flow noticed/ })
  ).toHaveCount(0)
  await orbAt(page, "ambient")
  await expectOrbClear(page, [noticed(page), yourFlow(page)])
  expect(sessions.filter((s) => s.method === "POST")).toHaveLength(0)
  expect(sockets).toBe(0)
})

test("Review priorities opens the Flow noticed scene; a Your Flow card opens its scene", async ({
  page,
}) => {
  await page.goto("/")
  await noticed(page).getByRole("link", { name: sc.noticed.review }).click()
  await expect(page).toHaveURL(/\/scenes\/noticed$/)
  await expect(
    page.getByRole("heading", { level: 1, name: "Flow noticed" })
  ).toBeVisible()
  await orbAt(page, "compact")
  await page.goBack()
  await yourFlow(page)
    .getByRole("link", { name: /Stock Watch/ })
    .click()
  await expect(
    page.getByRole("heading", { level: 1, name: "Stock Watch" })
  ).toBeVisible()
  await expect(orb(page)).toHaveAttribute("data-orb", "compact")
})

test("an operations role never sees sales values: whole scenes hide, partial ones say restricted", async ({
  page,
  api,
}) => {
  sceneWorld(api, { role: "ops" })
  await page.goto("/")
  await expect(
    yourFlow(page).getByRole("link", { name: /Stock Watch/ })
  ).toBeVisible()
  // Sales Perspective and Order Focus are restricted in full: no preview at all.
  await expect(
    yourFlow(page).getByRole("link", { name: /Sales Perspective/ })
  ).toHaveCount(0)
  await expect(
    yourFlow(page).getByRole("link", { name: /Order Focus/ })
  ).toHaveCount(0)
  await expect(page.getByText("€")).toHaveCount(0)
  await yourFlow(page)
    .getByRole("link", { name: /Morning Pulse/ })
    .click()
  await expect(page.locator("[data-restricted]").first()).toContainText(
    sc.restricted
  )
  await expect(page.getByText("€")).toHaveCount(0)
  await expect(
    page.locator("[data-metric-card]").filter({ hasText: /^0$/ })
  ).toHaveCount(0)
  expect(api.callsTo("GET", "/workspace/w1/sales-summary")).toHaveLength(0)
  expect(api.callsTo("GET", "/workspace/w1/orders")).toHaveLength(0)
})

test.describe("periods follow the workspace's zone, not the browser's", () => {
  test.use({ timezoneId: "America/Santiago" })
  test("at 22:30 UTC Dar es Salaam is already on the next day", async ({
    page,
  }) => {
    const asked: string[] = []
    page.on("request", (r) => {
      if (r.url().includes("/sales-summary"))
        asked.push(new URL(r.url()).searchParams.get("from") ?? "")
    })
    await page.clock.setFixedTime(new Date("2026-11-11T22:30:00Z"))
    await page.goto("/")
    await expect(
      yourFlow(page).getByRole("link", { name: /Morning Pulse/ })
    ).toContainText("€12,840.00")
    // Browser (Santiago) says the 11th; the workspace zone the API reports says the 12th, which
    // starts at 21:00 UTC (sales-summary takes instants).
    expect(asked).toContain("2026-11-11T21:00:00.000Z")
    expect(asked).not.toContain("2026-11-11T03:00:00.000Z")
  })
})
