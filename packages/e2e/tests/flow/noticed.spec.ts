// F2: Flow noticed prioritisation over a SIMULATED API. Production test E; checklist B, S.
import { expect, test } from "./storefront-api"
import { HONEY, SPICES, TEA, sc, sceneWorld } from "./scenes-api"

// Every test signs in and gets the stand-in API, also those that never touch `api`.
test.beforeEach(({ api }) => void api)

test("acknowledge keeps the alert, snooze hides it, a cleared condition removes it, home follows", async ({
  page,
  api,
}) => {
  const world = sceneWorld(api)
  await page.goto("/scenes/noticed")
  const stock = page.locator('[data-group="stock.below_minimum"]')
  await expect(stock).toContainText(
    "3 products below their minimum stock levels"
  )
  await expect(stock).toContainText("3 open orders".replace("3", "8"))
  // Only the actions the API offers for this kind.
  await expect(
    page.locator('[data-group="orders.open"]').getByRole("button")
  ).toHaveCount(0)

  const honey = stock.locator(`[data-item="${HONEY}"]`)
  await honey.getByRole("button", { name: sc.noticed.acknowledge }).click()
  await expect(honey.locator("[data-acknowledged]")).toHaveText(
    sc.noticed.acknowledged
  )
  expect(api.callsTo("PUT", "/workspace/w1/attention/marks")[0]!.body).toEqual({
    kind: "stock.below_minimum",
    ref_id: HONEY,
    state: "acknowledged",
  })

  await stock
    .locator(`[data-item="${TEA}"]`)
    .getByRole("button", { name: sc.noticed.snoozeWeek })
    .click()
  await expect(stock.locator(`[data-item="${TEA}"]`)).toHaveCount(0)
  await expect(stock).toContainText(
    "2 products below their minimum stock levels"
  )

  // Stock raised above the minimum (the real fix happens through a reviewed action, F4).
  world.resolve(SPICES)
  await page.getByRole("button", { name: sc.refresh }).click()
  await expect(stock.locator(`[data-item="${SPICES}"]`)).toHaveCount(0)
  await expect(honey).toBeVisible()

  await page.goto("/")
  await expect(
    page.getByRole("region", { name: sc.noticed.label })
  ).toContainText("1 product below its minimum stock level")
})

test("nothing to notice is said calmly, not as an empty box", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/w1/attention/priorities", () => ({
    data: { generated_at: "2026-11-11T06:00:00Z", groups: [], restricted: [] },
  }))
  await page.goto("/")
  await expect(
    page.getByRole("region", { name: sc.noticed.label })
  ).toContainText(sc.noticed.calm)
  await page.goto("/scenes/noticed")
  await expect(page.getByText(sc.noticed.calm)).toBeVisible()
})

test.describe("priorities down", () => {
  test.use({
    allow: {
      responses: [
        { url: "/v1/workspace/w1/attention/priorities", status: 503 },
      ],
    },
  })
  test("a failed priorities call offers a retry inside the scene", async ({
    page,
    api,
  }) => {
    api.on("GET", "/workspace/w1/attention/priorities", () => ({
      status: 503,
      body: { message: "down" },
    }))
    await page.goto("/scenes/noticed")
    // The query retries a few times (backoff) before it says it failed.
    await expect(page.getByRole("button", { name: sc.retry })).toBeVisible({
      timeout: 15_000,
    })
  })
})
