import type { Page } from "@playwright/test"

import {
  catalogPage,
  expect,
  honey,
  NEXT_SINCE,
  seedTill,
  test,
  type TillRow,
} from "./fake-till"

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
  await page.getByLabel("Search products").fill(text)
  return page.getByRole("list", { name: "Search results" })
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
    api,
  }) => {
    void api // installs the fake API
    await seedCatalog(page)
    await seedTill(page)
    // Let the start-up delta land before the network goes.
    const settled = page.waitForResponse((r) =>
      r.url().includes("/till/catalog?page=1")
    )
    await page.goto("/sale")
    await settled
    await context.setOffline(true)
    await page.getByLabel("Scan or type a barcode").fill("8712345000035")
    await page.getByLabel("Scan or type a barcode").press("Enter")
    await expect(
      page.getByRole("region", { name: "Cart" }).getByText("Beeswax block")
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
  await page.getByLabel("Scan or type a barcode").fill("8712345000035")
  await page.getByLabel("Scan or type a barcode").press("Enter")
  await expect(
    page
      .getByRole("alert")
      .filter({ hasText: "No product with code 8712345000035." })
  ).toBeVisible()
})
