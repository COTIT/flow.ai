import { expect, expectFits, paged, test } from "./fake-api"

const lot = {
  id: "lot-jar",
  product_id: "prod-jar",
  code: "ILU-J-2026-01",
  public_code: "JR12JR34JR",
  status: "released",
  next_statuses: ["quarantine", "recalled"],
  produced_on: null,
  best_before: null,
  expires_on: null,
  origin: null,
  parent_lot: null,
  pack: null,
  received_from: null,
  created_at: "2026-11-01T08:00:00Z",
  updated_at: "2026-11-01T08:00:00Z",
}

const farmers = [
  { village: "Ilundo", farmers: 12, harvest_start: "2026-06-01", harvest_end: "2026-08-30" },
  // Under 3 farmers the API hides the count; no harvest dates recorded.
  { village: "Mbeya", farmers: null, harvest_start: null, harvest_end: null },
]

test.beforeEach(({ api }) => {
  api
    .on("GET", "/workspace/:ws/lots/:id", () => ({ data: lot }))
    .on("GET", "/workspace/:ws/lots/:id/events", () => paged([]))
    .on("GET", "/workspace/:ws/products/:id", () => ({
      data: { id: "prod-jar", name: "Ilundo honey 250 g" },
    }))
    .on("GET", "/workspace/:ws/lots/:id/passport", () => ({
      data: { draft: {}, published: null, published_at: null, version: 0 },
    }))
})

test("sales and returns that credited farmers, by group only", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/lots/:id/farmer-credits", () => ({
    ...paged([
      {
        order_id: "ord-7",
        order_number: "POS-0007",
        till_ref: "c-7",
        kind: "sale",
        jars: "3",
        at: "2026-11-01T09:00:00Z",
      },
      {
        order_id: "ord-8",
        order_number: "WEB-0008",
        till_ref: null,
        kind: "return",
        jars: "1",
        at: "2026-11-02T09:00:00Z",
      },
    ]),
    farmer_groups: 2,
    farmers,
  }))
  await page.goto("/lots/lot-jar")
  // The public page's village groups (OfficeLot rows, no bars or shares): never names.
  const credits = page.getByRole("region", { name: "Farmer credits" })
  const ilundo = credits.getByRole("listitem").filter({ hasText: "Ilundo" })
  await expect(ilundo).toContainText("IL")
  await expect(ilundo).toContainText("12 farmers · Jun 1, 2026 – Aug 30, 2026")
  const mbeya = credits.getByRole("listitem").filter({ hasText: "Mbeya" })
  await expect(mbeya).toContainText("Fewer than 3 farmers")
  await expect(credits.getByRole("status")).toHaveCount(0)
  await expect(
    page.getByRole("listitem").filter({ hasText: "POS-0007" }).getByText("Sale")
  ).toBeVisible()
  await expect(
    page
      .getByRole("listitem")
      .filter({ hasText: "WEB-0008" })
      .getByText("Return")
  ).toBeVisible()
  await expect(page.getByRole("link", { name: "POS-0007" })).toHaveAttribute(
    "href",
    "/orders/ord-7"
  )
  // The jars as the API states them, per sale; never summed.
  await expect(
    page.getByRole("listitem").filter({ hasText: "POS-0007" })
  ).toContainText("3 jars")
})

test("no lineage or village not shared: no groups, and no sales yet", async ({
  page,
}) => {
  await page.goto("/lots/lot-jar")
  await expect(
    page.getByRole("status").filter({
      hasText: "The public page shows no farmer groups for this batch.",
    })
  ).toBeVisible()
  await expect(
    page.getByText("No sales or returns of this batch yet.")
  ).toBeVisible()
})

test("lineage: packed from its parent batch, with the pack numbers as given", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/lots/:id", () => ({
    data: {
      ...lot,
      parent_lot: { id: "lot-raw", code: "AFM-RAW-01" },
      pack: { units: "10", raw_kg: "7.50" },
    },
  }))
  await page.goto("/lots/lot-jar")
  await expect(page.getByRole("link", { name: "AFM-RAW-01" })).toHaveAttribute(
    "href",
    "/lots/lot-raw"
  )
  await expect(page.getByText("10 units from 7.50 kg")).toBeVisible()
})

test("lineage: received from a partner's batch over a transfer", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/lots/:id", () => ({
    data: {
      ...lot,
      received_from: {
        transfer_id: "tr-1",
        from_lot_code: "ILU-H-2026-01",
        from_lot_public_code: "7K3M9Q2XRT",
      },
    },
  }))
  await page.goto("/lots/lot-jar")
  await expect(
    page.getByText(
      "Received from the partner's batch ILU-H-2026-01 (7K3M9Q2XRT)."
    )
  ).toBeVisible()
  await expect(
    page.getByRole("link", { name: "See transfers" })
  ).toHaveAttribute("href", "/supply/transfers")
})

test("lineage: the harvest's villages and their farmer counts", async ({ page, api }) => {
  api.on("GET", "/workspace/:ws/lots/:lot/farmer-credits", () => ({
    ...paged([]),
    farmer_groups: 2,
    farmers,
  }))
  await page.goto("/lots/lot-jar")
  const lineage = page.getByRole("region", { name: "Lineage" })
  await expect(lineage.getByText("Harvest from Ilundo")).toBeVisible()
  await expect(lineage.getByText("12 farmers")).toBeVisible()
  await expect(lineage.getByText("Harvest from Mbeya")).toBeVisible()
  await expect(lineage.getByText("Fewer than 3 farmers")).toBeVisible()
  await expect(lineage).not.toContainText("Made here")
  await expectFits(page)
})

test("lineage: a batch made here shows no lineage line", async ({ page }) => {
  await page.goto("/lots/lot-jar")
  await expect(page.getByText("Packed from batch")).toHaveCount(0)
  await expect(page.getByText(/Received from the partner/)).toHaveCount(0)
  await expect(page.getByRole("region", { name: "Lineage" })).toContainText(
    "Made here"
  )
})
