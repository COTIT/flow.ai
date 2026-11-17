import { expectNoRawValues } from "../../src/test"
import {
  apiAs,
  cred,
  expect,
  loginOffice,
  test,
  unique,
  urls,
} from "../../src/stack"

/** Upload a CSV through the real parse route, check it, and land on the review page. */
async function checkFile(
  page: import("@playwright/test").Page,
  kind: string,
  name: string,
  csv: string
) {
  await page.goto(`${urls.office}/settings/imports/new`)
  if (kind !== "New products") {
    await page.getByRole("combobox", { name: "What to import" }).click()
    await page.getByRole("option", { name: kind, exact: true }).click()
  }
  await page
    .getByLabel(/Spreadsheet/)
    .setInputFiles({ name, mimeType: "text/csv", buffer: Buffer.from(csv) })
  await expect(
    page.getByText(new RegExp(`rows found in ${name}`))
  ).toBeVisible()
  await page.getByRole("button", { name: "Check rows" }).click()
  await expect(page).toHaveURL(/\/settings\/imports\/[0-9a-f-]{36}$/)
}

test("products from a spreadsheet: only the ready rows are imported", async ({
  page,
  request,
}) => {
  const tag = unique()
  const owner = cred("office (AFM)", "owner")
  await loginOffice(page, owner)
  await checkFile(
    page,
    "New products",
    `products-${tag}.csv`,
    `Name,Price,Barcode\nImport rice ${tag},2.50,imp-${tag}-1\n,1.00,imp-${tag}-2\nImport beans ${tag},3.00,imp-${tag}-3\n`
  )
  await expect(
    page.getByText("2 ready · 1 need a fix · 0 imported")
  ).toBeVisible()
  await expect(
    page.getByText("1 rows need a fix and will not be imported.", {
      exact: false,
    })
  ).toBeVisible()
  await expectNoRawValues(page)

  await page.getByRole("button", { name: "Import 2 ready rows" }).click()
  await expect(page.getByText("Import finished: 2 rows imported.")).toBeVisible(
    { timeout: 30_000 }
  )

  const api = await apiAs(request, owner)
  const id = page.url().split("/").pop()!
  const applied = await api.get<
    { result_id: string; fields: { name: string } }[]
  >(api.ws(`/imports/${id}/rows?status=applied`))
  expect(applied.map((r) => r.fields.name).sort()).toEqual([
    `Import beans ${tag}`,
    `Import rice ${tag}`,
  ])
  for (const r of applied) {
    const product = await api.get<{ name: string }>(
      api.ws(`/products/${r.result_id}`)
    )
    expect(product.name).toBe(r.fields.name)
  }
  // The row without a name never became a product.
  const invalid = await api.get<{ status: string; result_id: string | null }[]>(
    api.ws(`/imports/${id}/rows?status=invalid`)
  )
  expect(invalid).toHaveLength(1)
  expect(invalid[0]!.result_id).toBeNull()
})

test("customers and suppliers from a spreadsheet; a bad email is shown in words", async ({
  page,
}) => {
  const tag = unique()
  await loginOffice(page, cred("office (AFM)", "owner"))
  await checkFile(
    page,
    "New customers and suppliers",
    `relations-${tag}.csv`,
    `Type,Company name,Email,Street,House number,Postcode,City,Country\n` +
      `business,Import Co ${tag},buyer-${tag}@example.test,Main,1,1000AA,Arusha,tz\n` +
      `business,Bad Mail ${tag},not-an-email,Main,2,1000AA,Arusha,tz\n`
  )
  await expect(
    page.getByText("1 ready · 1 need a fix · 0 imported")
  ).toBeVisible()
  await page.getByRole("button", { name: "Needs a fix" }).click()
  await expect(page.getByText(/valid email address/)).toBeVisible()
  await expectNoRawValues(page)
  await page.getByRole("button", { name: "Import 1 ready rows" }).click()
  await expect(page.getByText("Import finished: 1 rows imported.")).toBeVisible(
    { timeout: 30_000 }
  )
  await page.getByRole("button", { name: "Imported" }).click()
  await expect(
    page.getByText(`Import Co ${tag}`, { exact: false })
  ).toBeVisible()
})
