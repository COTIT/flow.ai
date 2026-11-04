import type { APIRequestContext } from "@playwright/test"

import { apiAs, cred, expect, loginOffice, stackAllow, test, unique, urls, workspaces } from "../../src/stack"

const catalogWith = (request: APIRequestContext, key: string) =>
  request.get(`${urls.api}/v1/storefront/products?page_size=1`, {
    headers: { Authorization: `Bearer ${key}` },
  })

test("office app key: the secret shows once, reads the catalog, and stops working once revoked", async ({
  page,
  request,
}) => {
  const name = `e2e key ${unique()}`
  await loginOffice(page, cred("office (AFM)", "owner"))
  await page.goto(`${urls.office}/settings/app-keys`)
  await page.getByRole("button", { name: "New app key" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill(name)
  await dialog.getByLabel("Kind").fill("shop")
  await dialog.getByLabel("Scopes").fill("catalog:read")
  await dialog.getByRole("button", { name: "Save" }).click()
  const secret = (await dialog.getByLabel("Secret").textContent())!.trim()
  expect(secret).toMatch(/^gsk_[0-9a-f]{64}$/)
  expect((await catalogWith(request, secret)).status()).toBe(200)

  await dialog.getByRole("button", { name: "I saved it" }).click()
  await expect(page.getByText(secret)).toHaveCount(0)
  const row = page.getByRole("row", { name: new RegExp(name) })
  await row.getByRole("button", { name: "Revoke" }).click()
  await page.getByRole("alertdialog").getByRole("button", { name: "Revoke" }).click()
  await expect(page.getByRole("status").filter({ hasText: "App key revoked." })).toBeVisible()
  await expect(row.getByText("Revoked", { exact: true })).toBeVisible()
  expect((await catalogWith(request, secret)).status()).toBe(401)
})

test.describe(() => {
  test.use({
    allow: {
      ...stackAllow,
      responses: [
        ...stackAllow.responses,
        { url: `${urls.office}/v1/workspace/${workspaces.AFM}/app-keys`, status: 422 },
        { url: `${urls.office}/v1/workspace/${workspaces.AFM}/webhook-endpoints`, status: 422 },
      ],
    },
  })

  test("office app key: a scope the API does not know is refused and no key is made", async ({ page, request }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const name = `e2e bad key ${unique()}`
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/settings/app-keys`)
    await page.getByRole("button", { name: "New app key" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill(name)
    await dialog.getByLabel("Kind").fill("shop")
    await dialog.getByLabel("Scopes").fill("orders:delete")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("Scopes")).toHaveAttribute("aria-invalid", "true")
    await expect(dialog.getByLabel("Secret")).toHaveCount(0)
    const keys = await owner.get<{ name: string }[]>(owner.ws("/app-keys?page_size=100"))
    expect(keys.map((k) => k.name)).not.toContain(name)
  })

  test("office webhook: create shows the secret once and lists it; a non-https URL and an unknown event are refused", async ({
    page,
    request,
  }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const url = `https://hooks.example.com/e2e/${unique()}`
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/settings/webhooks`)
    await page.getByRole("button", { name: "New endpoint" }).click()
    const dialog = page.getByRole("dialog")

    await dialog.getByLabel("URL").fill(url.replace("https:", "http:"))
    await dialog.getByLabel("Events").fill("stock.changed")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("URL")).toHaveAccessibleDescription("The url must be a public https URL.")

    await dialog.getByLabel("URL").fill(url)
    await dialog.getByLabel("Events").fill("stock.changed, order.teleported")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("Events")).toHaveAccessibleDescription(/Unknown event order\.teleported\./)

    await dialog.getByLabel("Events").fill("stock.changed, order.created")
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("Secret")).toHaveText(/^whsec_/)
    await page.keyboard.press("Escape")
    await expect(page.getByText(/whsec_/)).toHaveCount(0)
    await expect(page.getByRole("cell", { name: url, exact: true })).toBeVisible()

    const hooks = await owner.get<{ url: string; events: string[]; active: boolean }[]>(
      owner.ws("/webhook-endpoints?page_size=100")
    )
    const hook = hooks.find((h) => h.url === url)!
    expect(hook.active).toBe(true)
    // The API keeps events as a set; order is its own.
    expect([...hook.events].sort()).toEqual(["order.created", "stock.changed"])

    // Delete it again: the workspace allows at most 10 endpoints, and every run adds one.
    await page
      .getByRole("row")
      .filter({ has: page.getByRole("cell", { name: url, exact: true }) })
      .getByRole("button", { name: "Delete" })
      .click()
    await page.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click()
    await expect(page.getByRole("cell", { name: url, exact: true })).toHaveCount(0)
    const after = await owner.get<{ url: string }[]>(owner.ws("/webhook-endpoints?page_size=100"))
    expect(after.map((h) => h.url)).not.toContain(url)
  })
})
