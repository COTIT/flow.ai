import {
  apiAs,
  cred,
  expect,
  loginOffice,
  test,
  unique,
  urls,
} from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
import { farmApi, who } from "../field/field"

type Options = {
  harvests: {
    harvest_report_id: string
    farmer_name: string
    available_kg: string
  }[]
  raw_products: { id: string; name: string }[]
}
type Batch = {
  id: string
  lot: { id: string; code: string }
  lines: { share_id: string; farmer_name: string; kg: string }[]
}
type Correction = { id: string; status: string; net_kg: string }

// Gift Ngwale (Mwangeza) is the seeded farmer with verified coffee; the till chain spec buys
// 1 kg of hers, so this one buys from another Mwangeza farmer when there is one.
const gift = who("farmer", "0714003001")

// ops buy 2 kg, ops file "it was 1 kg", the RUAHA admin (a second person) approves in the
// Office: the raw lot's stock and the harvest's free kg follow. Nothing was sold from the lot
// yet, so no money moves (money re-splits are proven in the API suite).
test("a bought delivery corrected by one person and approved by another in the Office", async ({
  page,
  request,
}, info) => {
  test.setTimeout(120_000)
  const ops = await farmApi(request, cred("field (RUAHA)", "ops"))
  const admin = await apiAs(request, cred("office (RUAHA)", "admin"))
  const village = (await farmApi(request, gift)).me.memberships[0]!.village_id!
  const crops = await ops.get<{ id: string; code: string }[]>(
    "/farm/crops?page_size=100"
  )
  const coffee = crops.data.find((c) => c.code === "COFFEE")!.id
  const optionsPath = `/farm/supply-batches/options?village_id=${village}&crop_id=${coffee}`
  const options = (await ops.get<Options>(optionsPath)).data
  const usable = options.harvests.filter((h) => Number(h.available_kg) >= 2)
  // Each project its own harvest: both run at once and assert the harvest's free kg.
  const others = usable.filter((h) => h.farmer_name !== "Gift Ngwale")
  const harvest =
    others[info.project.name === "stack-mobile" ? 1 : 0] ?? usable[0]!
  const raw = options.raw_products.find((p) =>
    p.name.startsWith("Coffee cherries")
  )!
  const tzs = (
    await admin.get<{ id: string; asset_type: string }[]>(
      admin.ws("/currencies?page_size=100")
    )
  ).find((c) => c.asset_type === "TZS")!.id
  const code = `E2E-COR-${unique()}`.toUpperCase()
  const batch = (
    await ops.post<Batch>("/farm/supply-batches", {
      client_ref: crypto.randomUUID(),
      village_id: village,
      crop_id: coffee,
      code,
      raw_product_id: raw.id,
      price_per_kg: 800000,
      currency_id: tzs,
      lines: [{ harvest_report_id: harvest.harvest_report_id, kg: "2.00" }],
    })
  ).data
  const share = batch.lines[0]!
  const stock = async () =>
    (
      await admin.get<{ level: string }[]>(
        admin.ws(`/lots/${batch.lot.id}/stock`)
      )
    ).map((s) => s.level)
  const freeKg = async () =>
    Number(
      (await ops.get<Options>(optionsPath)).data.harvests.find(
        (h) => h.harvest_report_id === harvest.harvest_report_id
      )?.available_kg ?? 0
    )
  expect(await stock()).toEqual(["2.00"])
  const freeBefore = await freeKg()

  const filed = (
    await ops.post<Correction>(`/farm/supply-batches/${batch.id}/corrections`, {
      client_ref: crypto.randomUUID(),
      reason: `Scale read high ${code}`,
      lines: [{ share_id: share.share_id, kg: "1.00" }],
    })
  ).data
  expect(filed.status).toBe("pending")
  expect(await stock(), "nothing moves before approval").toEqual(["2.00"])

  await test.step("the admin reviews it in the Office and approves", async () => {
    await loginOffice(page, cred("office (RUAHA)", "admin"))
    await page.goto(`${urls.office}/supply/corrections`)
    const row = page
      .getByRole("row")
      .filter({ hasText: `Scale read high ${code}` })
    await expect(row).toContainText(`${share.farmer_name}: 2.00 → 1.00 kg`)
    await expect(row).toContainText("Waiting for approval")
    await row.getByRole("button", { name: "Review" }).click()
    const dialog = page.getByRole("dialog")
    await expect(
      dialog.getByText("Raw lot stock changes by -1.00 kg")
    ).toBeVisible()
    await expectNoRawValues(page)
    await dialog.getByRole("button", { name: "Approve" }).click()
    await expect(dialog).toBeHidden()
    await expect(row).toContainText("Approved")
  })

  expect(await stock(), "the raw lot lost the kg").toEqual(["1.00"])
  expect(await freeKg(), "the harvest has the kg back").toBeCloseTo(
    freeBefore + 1,
    2
  )
  const again = await ops.call(
    "POST",
    `/farm/supply-corrections/${filed.id}/approve`
  )
  expect(again.status, "decided once").toBe(409)
})
