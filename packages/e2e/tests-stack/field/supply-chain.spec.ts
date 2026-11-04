import { apiAs, cred, expect, loginOffice, openShift, pairTill, test, unique, urls } from "../../src/stack"
import { expectNoRawValues } from "../../src/test"
import { farmApi, signIn, who, type Passbook } from "./field"

// Gift Ngwale (seeded farmer login, Mwangeza) has verified coffee nobody has bought yet.
const gift = who("farmer", "0714003001")
const hamisi = who("ops", "hamisi.ops@ruaha-demo.test")

/** Forgets the Field session (it lives in Field's localStorage), so another user can sign in. */
async function signOutOfField(page: import("@playwright/test").Page) {
  await page.goto(`${urls.field}/login`)
  await page.evaluate(() => localStorage.clear())
}

type Options = { harvests: { harvest_report_id: string; farmer_name: string; available_kg: string }[] }
type Batch = { lot: { id: string; code: string } }
type Warehouse = { id: string; name: string }

// One farmer's harvest, sold once: from Field's Buy harvest to the AFM till and back to the
// farmer's passbook. Both projects would credit the same farmer at once, so one runs it; the
// screens on the way have their own 375 px checks.
test("a farmer's harvest bought in Field, shipped to AFM and sold at its till comes back as the farmer's credit", async ({ page, request }, info) => {
  test.skip(info.project.name !== "stack-desktop", "one chain per run: both projects would credit the same farmer")
  test.setTimeout(240_000)
  const code = `E2E-${unique()}`.toUpperCase()
  const ops = await farmApi(request, cred("field (RUAHA)", "ops"))
  const farmer = await farmApi(request, gift)
  const person = farmer.me.memberships[0]!.person_id!
  const village = farmer.me.memberships[0]!.village_id!
  const crops = await ops.get<{ id: string; code: string }[]>("/farm/crops?page_size=100")
  const coffee = crops.data.find((c) => c.code === "COFFEE")!.id
  const optionsPath = `/farm/supply-batches/options?village_id=${village}&crop_id=${coffee}`
  const before = (await ops.get<Options>(optionsPath)).data.harvests
  const harvest = before.find((h) => h.farmer_name === "Gift Ngwale")!
  const creditsBefore = ((await ops.get(`/farm/persons/${person}/passbook?page_size=100`)) as unknown as Passbook).data.map((l) => l.id)

  await test.step("ops buy 1 kg of the farmer's verified harvest in Field", async () => {
    await signIn(page, request, cred("field (RUAHA)", "ops"))
    await page.goto(`${urls.field}/ops/dispatch`)
    await page.getByTestId("batch-village").selectOption({ label: "Mwangeza" })
    await page.getByTestId("batch-crop").selectOption({ label: "Coffee" })
    await page.getByTestId("batch-product").selectOption({ label: "Coffee cherries (raw) (kg)" })
    for (const h of before) await page.getByTestId(`batch-kg-${h.harvest_report_id}`).fill(h === harvest ? "1.00" : "")
    await page.getByTestId("batch-code").fill(code)
    await page.getByTestId("batch-price").fill("8000")
    await page.getByTestId("batch-submit").click()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("batch-made")).toContainText(`Bought into lot ${code}.`)
    await expectNoRawValues(page)
    const after = (await ops.get<Options>(optionsPath)).data.harvests.find((h) => h.harvest_report_id === harvest.harvest_report_id)
    expect(Number(harvest.available_kg) - Number(after?.available_kg ?? 0), "kg left on the harvest").toBeCloseTo(1, 2)
  })

  const lot = (await ops.get<Batch[]>("/farm/supply-batches?page_size=100")).data.find((b) => b.lot.code === code)!.lot
  const ruaha = await apiAs(request, cred("office (RUAHA)", "admin"))

  await test.step("the bought kg are in the farm's store at once, so the lot can ship", async () => {
    const stock = await ruaha.get<{ level: string }[]>(ruaha.ws(`/lots/${lot.id}/stock`))
    expect(stock.map((s) => s.level)).toEqual(["1.00"])
    const dispatchable = await ops.get<{ lots: { code: string }[] }>("/farm/dispatch/options")
    expect(dispatchable.data.lots.map((l) => l.code)).toContain(code)
  })

  await test.step("ops dispatch it to the AFM shop in Field", async () => {
    await page.goto(`${urls.field}/ops/dispatch`)
    await page.getByTestId("dispatch-link").selectOption({ label: "AFM Shop" })
    await page.getByTestId("dispatch-lot").selectOption({ label: code })
    await page.getByTestId("dispatch-warehouse").selectOption({ label: "Ilundo store" })
    await page.getByTestId("dispatch-quantity").fill("1.00")
    await page.getByTestId("dispatch-send").click()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("dispatch-sent")).toBeVisible()
    await expect(page.getByTestId("supply-transfer-row").filter({ hasText: code })).toBeVisible()
    await expectNoRawValues(page)
  })

  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const product = `e2e Ruaha coffee ${unique()}`
  await owner.post(owner.ws("/products"), { name: product, price: "4.00" })
  const { register } = await pairTill(page, request)

  await test.step("AFM receives the transfer in the office, into the till's store", async () => {
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/supply/transfers`)
    await page.getByRole("button", { name: `Receive batch ${code}` }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Product").selectOption({ label: product })
    await dialog.getByLabel("New batch code").fill(`AFM-${code}`)
    const warehouses = await owner.get<Warehouse[]>(owner.ws("/warehouses?page_size=100"))
    await dialog.getByLabel("Warehouse").selectOption({ label: warehouses.find((w) => w.id === register.warehouse_id)!.name })
    await dialog.getByRole("button", { name: "Receive" }).click()
    // The office opens the new AFM lot, traced to the farm's lot.
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(`AFM-${code}`)
    await expect(page.getByText(`Received from the partner's batch ${code}`)).toBeVisible()
  })

  await test.step("the AFM till sells it", async () => {
    await openShift(page)
    await page.goto(`${urls.till}/sale`)
    await page.getByLabel("Scan a barcode or type a name").fill(product)
    await page.getByRole("list", { name: "Products" }).getByRole("button", { name: new RegExp(product) }).click()
    await page.getByRole("button", { name: /^Charge/ }).click()
    const sheet = page.getByRole("dialog", { name: "Take payment" })
    await sheet.getByRole("button", { name: "Cash" }).click()
    await sheet.getByRole("button", { name: "Exact" }).click()
    await sheet.getByRole("button", { name: /^Book sale/ }).click()
    await expect(page.getByRole("dialog", { name: "Done" })).toBeVisible()
  })

  await test.step("the farmer sees the sale credit in their Field passbook", async () => {
    // 1 kg at TZS 8,000.00 per kg, all of it Gift's.
    const book = (await ops.get(`/farm/persons/${person}/passbook?page_size=100`)) as unknown as Passbook
    const fresh = book.data.filter((l) => !creditsBefore.includes(l.id))
    expect(fresh.map((l) => [l.kind, l.amount.currency, l.amount.minor])).toEqual([["sale_credit", "TZS", 800000]])
    await signOutOfField(page)
    await signIn(page, request, gift)
    await expect(page.getByTestId("farm-home-passbook")).toBeVisible()
    await page.goto(`${urls.field}/farm/passbook`)
    // Gift reads Swahili (the account's language): "Mapato ya mauzo" is "Sale credit".
    await expect(page.getByTestId("passbook-line").first()).toContainText("Mapato ya mauzo")
    await expect(page.getByTestId("passbook-line").first()).toContainText("TSh 8,000.00")
    await expectNoRawValues(page)
  })

  await test.step("ops pay the farmer out of it in Field", async () => {
    // Hamisi, not Asha: Asha verified this household (four eyes).
    await signOutOfField(page)
    await signIn(page, request, hamisi)
    await page.goto(`${urls.field}/officer/people/${person}`)
    await page.getByTestId("pay-out-TZS").click()
    await page.getByTestId("payout-amount").fill("1000")
    await page.getByTestId("payout-id-type").selectOption("nida")
    await page.getByTestId("payout-name-confirmed").check()
    await page.getByTestId("confirm-dialog-confirm").click()
    await expect(page.getByTestId("payout-done")).toContainText("1,000.00")
    await expectNoRawValues(page)
  })
})
