import type { APIRequestContext } from "@playwright/test"

import {
  apiAs,
  cred,
  expect,
  loginField,
  loginOffice,
  stackAllow,
  test,
  unique,
  urls,
  workspaces,
} from "../../src/stack"

/** The API's status for `path` as one member, without the throw `apiAs` does on >= 400. */
async function statusAs(
  request: APIRequestContext,
  token: string,
  path: string
) {
  const r = await request.get(`${urls.api}/v1${path}`, {
    headers: { Authorization: `Bearer ${token}` },
  })
  return r.status()
}

test("neither account reads the other's records through the API", async ({
  request,
}) => {
  const afm = await apiAs(request, cred("office (AFM)", "owner"))
  const ruaha = await apiAs(request, cred("office (RUAHA)", "admin"))
  const { product_id: afmProduct } = await afm.post<{ product_id: string }>(
    afm.ws("/products"),
    {
      name: `e2e afm private ${unique()}`,
      price: "3.00",
    }
  )
  const { product_id: ruahaProduct } = await ruaha.post<{ product_id: string }>(
    ruaha.ws("/products"),
    {
      name: `e2e ruaha private ${unique()}`,
      price: "3.00",
    }
  )

  // Each one's own record, for the baseline.
  expect(
    await statusAs(
      request,
      afm.token,
      `/workspace/${workspaces.AFM}/products/${afmProduct}`
    )
  ).toBe(200)
  expect(
    await statusAs(
      request,
      ruaha.token,
      `/workspace/${workspaces.RUAHA}/products/${ruahaProduct}`
    )
  ).toBe(200)

  // The other account's workspace, list and record: 404, so not even existence leaks.
  for (const [who, other, record] of [
    [afm, workspaces.RUAHA, ruahaProduct],
    [ruaha, workspaces.AFM, afmProduct],
  ] as const) {
    expect(
      await statusAs(request, who.token, `/workspace/${other}/products`)
    ).toBe(404)
    expect(
      await statusAs(
        request,
        who.token,
        `/workspace/${other}/products/${record}`
      )
    ).toBe(404)
    // The other's record id under one's own workspace is not found either.
    expect(
      await statusAs(request, who.token, `${who.ws("/products")}/${record}`)
    ).toBe(404)
  }
  for (const path of [
    "/app-keys",
    "/webhook-endpoints",
    "/invites",
    "/users/teams",
  ]) {
    expect(
      await statusAs(
        request,
        afm.token,
        `/workspace/${workspaces.RUAHA}${path}`
      ),
      path
    ).toBe(404)
    expect(
      await statusAs(
        request,
        ruaha.token,
        `/workspace/${workspaces.AFM}${path}`
      ),
      path
    ).toBe(404)
  }
})

// The AFM product is made before the guard reads `allow`, so its 404s can be listed exactly.
const byUrl = test.extend<{ afmProduct: { id: string; name: string } }>({
  afmProduct: async ({ request }, use) => {
    const afm = await apiAs(request, cred("office (AFM)", "owner"))
    const name = `e2e afm by url ${unique()}`
    const { product_id: id } = await afm.post<{ product_id: string }>(
      afm.ws("/products"),
      {
        name,
        price: "3.00",
      }
    )
    await use({ id, name })
  },
  allow: async ({ afmProduct }, use) =>
    use({
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        // Every call the product page makes for another account's record answers 404.
        ...PRODUCT_CALLS.map((tail) => ({
          url: `${urls.office}/v1/workspace/${workspaces.RUAHA}/products/${afmProduct.id}${tail}`,
          status: 404,
        })),
      ],
    }),
})
const PRODUCT_CALLS = [
  "",
  "/stock-with-bins",
  "/stock-movements?page=1",
  "/variants",
]

byUrl(
  "the other account's product by URL in the office is not found",
  async ({ page, afmProduct }) => {
    await loginOffice(page, cred("office (RUAHA)", "admin"))
    await page.goto(`${urls.office}/products/${afmProduct.id}`)
    await expect(page.getByText("Not found", { exact: true })).toBeVisible()
    await expect(
      page.getByRole("link", { name: "Back to the list" })
    ).toBeVisible()
    await expect(page.getByText(afmProduct.name)).toHaveCount(0)
    // A readable not-found state, no raw status code on screen.
    await expect(page.getByText(/\b404\b/)).toHaveCount(0)
  }
)

test("the account switcher offers only one's own accounts", async ({
  page,
  request,
}) => {
  const ruaha = await apiAs(request, cred("office (RUAHA)", "admin"))
  const mine =
    await ruaha.get<{ id: string; display_name: string }[]>("/me/workspaces")
  expect(mine.map((w) => w.id)).not.toContain(workspaces.AFM)
  await loginOffice(page, cred("office (RUAHA)", "admin"))
  const menu = page.getByRole("button", { name: "Open menu" })
  if (await menu.isVisible()) await menu.click()
  await page.getByRole("combobox", { name: "Workspace" }).click()
  const offered = await page.getByRole("option").allTextContents()
  expect(offered.sort()).toEqual(mine.map((w) => w.display_name).sort())
  expect(offered).toContain("Ruaha Coffee Co-op")
})

test("an office owner of one account has no farm role in the other's Field", async ({
  page,
}) => {
  await loginField(page, cred("office (AFM)", "owner"))
  await expect(page).toHaveURL(`${urls.field}/no-access`)
  await expect(page.getByText("No access yet")).toBeVisible()
})
