import { cachedUrls, expectInstallable, waitForServiceWorker } from "../../src/pwa"
import { expectNoHorizontalScroll } from "../../src/test"
import { expect, honey, seedTill, test, WS } from "../till/fake-till"

const REF = "00000000-0000-4000-8000-000000000002"

test("can be added to the home screen as 'Till', opening on the sale screen", async ({ page }) => {
  await page.goto("/setup")
  await expectInstallable(page, { name: "Genesy Till", shortName: "Till", startUrl: "/sale" })
})

test.describe("opened with no connection", () => {
  // The replay first asks whether the server already has the sale.
  test.use({
    allow: { responses: [{ url: `/v1/workspace/${WS}/till/sales/${REF}`, status: 404 }] },
  })

  test("starts on the sale screen, keeps a sale, and sends it once back online", async ({
    page,
    context,
    api,
  }) => {
    let booked = false
    api
      .on("GET", "/workspace/:ws/till/sales/:ref", () =>
        booked ? { data: { client_ref: REF, number: "1043" } } : { status: 404, body: { message: "Not found." } }
      )
      .on("POST", "/workspace/:ws/till/sales", () => {
        booked = true
        return { status: 201, body: { data: { client_ref: REF, number: "1043" } } }
      })
    await page.addInitScript((ref) => {
      crypto.randomUUID = () => ref as `${string}-${string}-${string}-${string}-${string}`
    }, REF)
    await seedTill(page)

    // Opened once online after install: the service worker keeps the till screens.
    await page.goto("/sale")
    await expect(page.getByRole("button", { name: new RegExp(honey.name) })).toBeVisible()
    await waitForServiceWorker(page)

    // Closed, then opened again with no network: the app still starts.
    await context.setOffline(true)
    await page.reload()
    await expect(page.getByText("Offline — sales are kept on this device")).toBeVisible()
    await page.getByRole("button", { name: new RegExp(honey.name) }).click()
    await page.getByRole("button", { name: /^Charge/ }).click()
    for (const d of "3000") await page.getByRole("button", { name: d, exact: true }).click()
    await page.getByRole("button", { name: /^Book sale/ }).click()
    await expect(page.getByRole("dialog", { name: "Saved offline" })).toBeVisible()
    await page.getByRole("button", { name: "New sale" }).click()
    await expect(page.getByText("Offline · 1 to sync")).toBeVisible()
    await expectNoHorizontalScroll(page)

    // Another till screen opens offline too, from the till's own link.
    await page.getByRole("link", { name: "Offline · 1 to sync" }).click()
    await expect(page.getByRole("heading", { name: "Offline sales" })).toBeVisible()
    await expect(page.getByText("Cash · ≈ €12.50 estimate")).toBeVisible()
    expect(api.callsTo("POST", `/workspace/${WS}/till/sales`)).toHaveLength(0)

    await context.setOffline(false)
    await expect.poll(() => api.callsTo("POST", `/workspace/${WS}/till/sales`).length).toBe(1)
  })
})

test("keeps no API answer on the device", async ({ page }) => {
  await seedTill(page)
  await page.goto("/sale")
  await waitForServiceWorker(page)
  const urls = await cachedUrls(page)
  expect(urls.some((u) => new URL(u).pathname === "/sale")).toBe(true)
  expect(urls.filter((u) => new URL(u).pathname.startsWith("/v1"))).toEqual([])
})
