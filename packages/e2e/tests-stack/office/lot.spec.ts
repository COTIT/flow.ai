import { type Api, apiAs, cred, expect, loginOffice, stackAllow, test, unique, urls, workspaces } from "../../src/stack"

type Lot = { id: string; code: string; public_code: string; status: string }

/**
 * Types into the passport form. The form is editable before its saved draft arrives and the
 * draft then overwrites it (seen on a phone under load), so type again until it sticks.
 */
async function typeDraft(page: import("@playwright/test").Page, label: string, value: string) {
  await expect(async () => {
    await page.getByLabel(label).fill(value)
    await expect(page.getByRole("button", { name: "Save draft" })).toBeEnabled({ timeout: 1_000 })
  }).toPass()
}

/** A stock-tracked product of the test's own: lots can only be made for those. */
async function trackedProduct(owner: Api, tag: string) {
  const name = `e2e lot honey ${tag}`
  const { product_id } = await owner.post<{ product_id: string }>(owner.ws("/products"), {
    name,
    price: "6.00",
    has_stock: true,
  })
  return { id: product_id, name }
}

test("office batch: create, publish its passport, recall it; the public page follows", async ({ page, request }) => {
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const tag = unique()
  const product = await trackedProduct(owner, tag)
  const code = `E2E-${tag}`.toUpperCase()
  const title = `Ilundo honey ${tag}`

  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/lots`)
  await page.getByRole("button", { name: "New batch" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Product").selectOption({ label: product.name })
  await dialog.getByLabel("Batch code").fill(code)
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(page.getByRole("status").filter({ hasText: "Batch created." })).toBeVisible()
  await expect(page.getByRole("heading", { name: code })).toBeVisible()
  const lotId = new URL(page.url()).pathname.split("/").at(-1)!
  const lot = await owner.get<Lot>(owner.ws(`/lots/${lotId}`))
  expect(lot).toMatchObject({ code, status: "released" })

  await page.getByRole("link", { name: "Edit passport" }).click()
  await typeDraft(page, "Title", title)
  await page.getByRole("button", { name: "Save draft" }).click()
  await expect(page.getByRole("button", { name: "Publish v1" })).toBeEnabled()
  await page.getByRole("button", { name: "Publish v1" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Publish" }).click()
  await expect(page.getByText("Published v1", { exact: true })).toBeVisible()

  // The public page, as a jar's QR link opens it.
  await page.goto(`${urls.shop}/b/${lot.public_code}`)
  await expect(page.getByRole("heading", { name: title })).toBeVisible()
  await expect(page.getByRole("alert").filter({ hasText: "This batch has been recalled" })).toHaveCount(0)

  await recall(page, lotId)
  expect((await owner.get<Lot>(owner.ws(`/lots/${lotId}`))).status).toBe("recalled")
  // The public API answers the recall at once; the shop page is checked in the test below.
  const pub = await request.get(`${urls.api}/v1/public/lots/${lot.public_code}`)
  expect(((await pub.json()) as { data: { status: string } }).data.status).toBe("recalled")
})

/** Recalls a batch from its office page and waits for the new status. */
async function recall(page: import("@playwright/test").Page, lotId: string) {
  await page.goto(`${urls.office}/lots/${lotId}`)
  await page.getByRole("button", { name: "Recall this batch…" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Recall", exact: true }).click()
  await expect(page.getByRole("region", { name: "Status" }).getByText("Recalled", { exact: true })).toBeVisible()
}

test("office batch: a recall shows on the public page at once", async ({ page, request }) => {
  test.fail(
    true,
    "BUG: the shop keeps storefront replies 60 s (apps/shop/lib/storefront.ts:48 revalidate: 60; getLot :95 uses it), so a recalled batch's QR page keeps showing it as safe after the recall (e2e-office)"
  )
  const owner = await apiAs(request, cred("office (AFM)", "owner"))
  const tag = unique()
  const product = await trackedProduct(owner, tag)
  const lot = await owner.post<Lot>(owner.ws("/lots"), { product_id: product.id, code: `E2E-${tag}`.toUpperCase() })
  await owner.put(owner.ws(`/lots/${lot.id}/passport`), { title: `Recall me ${tag}` })
  await owner.post(owner.ws(`/lots/${lot.id}/passport/publish`))
  // A shopper scans the jar before the recall.
  await page.goto(`${urls.shop}/b/${lot.public_code}`)
  await expect(page.getByRole("heading", { name: `Recall me ${tag}` })).toBeVisible()

  await loginOffice(page, cred("office (AFM)", "owner"))
  await recall(page, lot.id)

  // And again after it: the warning must be there now.
  await page.goto(`${urls.shop}/b/${lot.public_code}`)
  await expect(
    page.getByRole("alert").filter({ hasText: "This batch has been recalled" })
  ).toContainText("Please don't eat it, and contact us for a refund or replacement.")
})

test.describe(() => {
  // Made before the test so the allowed 404 and 422 can name it (allow is read as the test starts).
  let lot: Lot
  test.beforeAll(async ({ request }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const tag = unique()
    const product = await trackedProduct(owner, tag)
    lot = await owner.post<Lot>(owner.ws("/lots"), { product_id: product.id, code: `E2E-${tag}`.toUpperCase() })
  })
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          { url: `${urls.shop}/b/${lot.public_code}`, status: 404 },
          { url: `${urls.office}/v1/workspace/${workspaces.AFM}/lots/${lot.id}/passport/publish`, status: 422 },
        ],
      }),
  })

  test("office batch: an unpublished passport is not public; publishing without a title is refused", async ({
    page,
  }) => {
    await page.goto(`${urls.shop}/b/${lot.public_code}`)
    await expect(page.getByRole("heading", { name: "We can't find this batch" })).toBeVisible()

    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/lots/${lot.id}/passport`)
    await typeDraft(page, "Introduction", "No title yet.")
    await page.getByRole("button", { name: "Save draft" }).click()
    await expect(page.getByRole("button", { name: "Publish v1" })).toBeEnabled()
    await page.getByRole("button", { name: "Publish v1" }).click()
    await page.getByRole("alertdialog").getByRole("button", { name: "Publish" }).click()
    await expect(page.getByLabel("Title")).toHaveAccessibleDescription(
      "The passport needs a title before it can be published."
    )
    await expect(page.getByText("Published v1", { exact: true })).toHaveCount(0)

    // Still not public.
    await page.goto(`${urls.shop}/b/${lot.public_code}`)
    await expect(page.getByRole("heading", { name: "We can't find this batch" })).toBeVisible()
  })
})
