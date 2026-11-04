import type { Page } from "@playwright/test"

import {
  catalogPage,
  expect,
  honey,
  NEXT_SINCE,
  seedTill,
  serveQuote,
  test,
  type FakeApi,
  type TillRow,
} from "./fake-till"
import { skipRetries } from "./look"

const wax: TillRow = {
  ...honey,
  id: "p-wax",
  name: "Beeswax block",
  ean13: "8712345000035",
}
const SINCE = "2026-11-01T08:00:00Z"

/** A device copy from an earlier sync: honey and wax, synced up to SINCE. */
async function seedCatalog(page: Page) {
  const value = JSON.stringify({
    state: {
      registerId: "reg-1",
      products: { [honey.id]: honey, [wax.id]: wax },
      since: SINCE,
    },
    version: 0,
  })
  await page.addInitScript(
    (v) => localStorage.setItem("genesy.till.catalog", v),
    value
  )
}

async function searchFor(page: Page, text: string) {
  await page.getByLabel("Scan a barcode or type a name").fill(text)
  return page.getByRole("list", { name: "Products" })
}

test("the first sync reads every page of the catalog", async ({
  page,
  api,
}) => {
  const asked: (string | null)[] = []
  api.on("GET", "/workspace/:ws/till/catalog", ({ query }) => {
    asked.push(query.get("updated_since"))
    const n = Number(query.get("page") ?? 1)
    return catalogPage([honey, wax], n, 1)
  })
  await seedTill(page)
  await page.goto("/sale")
  await expect(
    (await searchFor(page, "bees")).getByText("Beeswax block")
  ).toBeVisible()
  expect(asked).toEqual([null, null])
  const saved = await page.evaluate(() =>
    localStorage.getItem("genesy.till.catalog")
  )
  expect(JSON.parse(saved!).state.since).toBe(NEXT_SINCE)
})

test("later syncs ask only for changes and drop what is no longer sellable", async ({
  page,
  api,
}) => {
  const asked: (string | null)[] = []
  api.on("GET", "/workspace/:ws/till/catalog", ({ query }) => {
    asked.push(query.get("updated_since"))
    return catalogPage([
      { ...honey, name: "Ilundo honey 1 kg" },
      { ...wax, sellable: false },
    ])
  })
  await seedCatalog(page)
  await seedTill(page)
  await page.goto("/sale")

  const results = await searchFor(page, "honey")
  await expect(results.getByText("Ilundo honey 1 kg")).toBeVisible()
  expect(asked).toEqual([SINCE])
  await expect((await searchFor(page, "bees")).getByRole("button")).toHaveCount(
    0
  )
})

test.describe("offline scan", () => {
  test("a scan without network finds the product in the device copy", async ({
    page,
    context,
  }) => {
    await seedCatalog(page)
    await seedTill(page)
    // Let the start-up delta land before the network goes.
    const settled = page.waitForResponse((r) =>
      r.url().includes("/till/catalog?page=1")
    )
    await page.goto("/sale")
    await settled
    await context.setOffline(true)
    await page.getByLabel("Scan a barcode or type a name").fill("8712345000035")
    await page.getByLabel("Scan a barcode or type a name").press("Enter")
    await expect(
      page
        .getByRole("complementary", { name: "Basket" })
        .getByText("Beeswax block")
    ).toBeVisible()
  })
})

test("a code the API doesn't know is reported, even if an old copy had it", async ({
  page,
  api,
}) => {
  // The device copy still has wax, but the server answers the scan with nothing.
  api.on("GET", "/workspace/:ws/till/catalog", ({ query }) =>
    catalogPage(query.get("code") ? [] : [honey, wax])
  )
  await seedCatalog(page)
  await seedTill(page)
  await page.goto("/sale")
  await page.getByLabel("Scan a barcode or type a name").fill("8712345000035")
  await page.getByLabel("Scan a barcode or type a name").press("Enter")
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "No product with code 8712345000035." })
  ).toBeVisible()
})

test("every minute the till asks for changes, so an office price edit reaches the tiles", async ({
  page,
  api,
}) => {
  const asked: (string | null)[] = []
  let price = { unit_price: 1147, unit_price_gross: 1250 }
  api.on("GET", "/workspace/:ws/till/catalog", ({ query }) => {
    if (query.get("code")) return catalogPage([])
    asked.push(query.get("updated_since"))
    return catalogPage([{ ...honey, ...price }])
  })
  await page.clock.install()
  await seedTill(page)
  await page.goto("/sale")
  const tile = page
    .getByRole("list", { name: "Products" })
    .getByRole("button", { name: /Ilundo honey 500 g/ })
  await expect(tile).toContainText("€12.50")
  expect(asked).toEqual([null])

  // The office changes the price; the till's copy is a minute old at most.
  price = { unit_price: 1239, unit_price_gross: 1350 }
  await page.clock.fastForward(30_000)
  await expect(tile).toContainText("€12.50")
  await page.clock.fastForward(30_000)
  await expect(tile).toContainText("€13.50")
  expect(asked).toEqual([null, NEXT_SINCE])
})

test.describe("catalog unavailable", () => {
  const url = "/v1/workspace/w1/till/catalog?page=1&page_size=100"
  test.use({
    allow: {
      responses: [
        { url, status: 503 },
        { url: `${url}&updated_since=${SINCE}`, status: 503 },
      ],
    },
  })

  test("with no copy on the device, the sale screen says the list couldn't load", async ({
    page,
    api,
  }) => {
    serveDown(api)
    await page.clock.install()
    await seedTill(page)
    await page.goto("/sale")
    await skipRetries(
      page,
      page.getByRole("alert").filter({
        hasText: "Couldn't load the product list. Check the connection.",
      })
    )
    await expect(
      page.getByRole("button", { name: /Ilundo honey/ })
    ).toHaveCount(0)
  })

  test("with a copy, the till keeps selling from it and says it is not fresh", async ({
    page,
    api,
  }) => {
    serveDown(api)
    serveQuote(api, [honey, wax])
    await page.clock.install()
    await seedCatalog(page)
    await seedTill(page)
    await page.goto("/sale")
    await skipRetries(
      page,
      page.getByRole("status").filter({
        hasText:
          "Couldn't refresh the product list. Showing the copy on this device.",
      })
    )
    await page
      .getByRole("list", { name: "Products" })
      .getByRole("button", { name: /Beeswax block/ })
      .click()
    await expect(
      page
        .getByRole("complementary", { name: "Basket" })
        .getByText("Beeswax block")
    ).toBeVisible()
  })
})

/** The catalog answers 503 busy, every time. */
function serveDown(api: FakeApi) {
  api.on("GET", "/workspace/:ws/till/catalog", () => ({
    status: 503,
    body: { message: "The server is busy, please try again." },
  }))
}

test.describe("session ends during a sync", () => {
  test.use({
    allow: {
      responses: [
        {
          url: "/v1/workspace/w1/till/catalog?page=2&page_size=100",
          status: 401,
        },
      ],
    },
  })

  test("a 401 on a later catalog page returns to the PIN screen and says why", async ({
    page,
    api,
  }) => {
    // Page 1 answers, then the device key or session dies before page 2.
    api.on("GET", "/workspace/:ws/till/catalog", ({ query }) =>
      query.get("page") === "2"
        ? { status: 401, body: { message: "Unauthenticated." } }
        : catalogPage([honey, wax], 1, 1)
    )
    await seedTill(page)
    await page.goto("/sale")
    await expect(page).toHaveURL("/login")
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: "Your session ended. Log in again to continue." })
    ).toBeVisible()
  })
})
