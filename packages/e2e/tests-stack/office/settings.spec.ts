import type { APIRequestContext } from "@playwright/test"

import { expectNoRawValues } from "../../src/test"
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
  await dialog.getByLabel("Kind").selectOption({ label: "Web shop" })
  await dialog.getByRole("checkbox", { name: "Read catalogue" }).check()
  await dialog.getByRole("button", { name: "Save" }).click()
  const secret = (await dialog.getByLabel("Secret").textContent())!.trim()
  expect(secret).toMatch(/^gsk_[0-9a-f]{64}$/)
  expect((await catalogWith(request, secret)).status()).toBe(200)

  await dialog.getByRole("button", { name: "I saved it" }).click()
  await expect(page.getByText(secret)).toHaveCount(0)
  const row = page.getByRole("row", { name: new RegExp(name) })
  // Kind and scope as words, never the API's codes.
  await expect(row).toContainText("Web shop")
  await expect(row).toContainText("Read catalogue")
  await expectNoRawValues(page)

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

  test("office app key: only known scopes are offered, as words; none ticked or an unknown one is refused", async ({
    page,
    request,
  }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    // A scope the form cannot offer, sent straight to the API: 422 and no key.
    const bad = `e2e bad key ${unique()}`
    await expect(
      owner.post(owner.ws("/app-keys"), { name: bad, kind: "shop", scopes: ["orders:delete"] })
    ).rejects.toThrow(/: 422 /)

    const name = `e2e no scope ${unique()}`
    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/settings/app-keys`)
    await page.getByRole("button", { name: "New app key" }).click()
    const dialog = page.getByRole("dialog")
    const scopes = dialog.getByRole("group", { name: "Scopes" })
    // The issuable scopes only (till login is the till's own), each by its name.
    await expect(scopes.getByRole("checkbox")).toHaveCount(2)
    await expect(scopes.getByRole("checkbox", { name: "Read catalogue" })).toBeVisible()
    await expect(scopes.getByRole("checkbox", { name: "Take web orders" })).toBeVisible()
    await expect(dialog.getByLabel("Kind").locator("option")).toHaveText([
      "Choose what the key is for",
      "Web shop",
      "Till",
      "Field app",
      "Partner",
    ])
    await expectNoRawValues(page, { within: "[role=dialog]" })

    await dialog.getByLabel("Name").fill(name)
    await dialog.getByLabel("Kind").selectOption({ label: "Web shop" })
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(scopes).toHaveAccessibleDescription("The scopes field must list 1 to 20 scopes.")
    await expect(dialog.getByLabel("Secret")).toHaveCount(0)

    const keys = await owner.get<{ name: string }[]>(owner.ws("/app-keys?page_size=100"))
    expect(keys.map((k) => k.name)).not.toContain(name)
    expect(keys.map((k) => k.name)).not.toContain(bad)
  })

  test("office webhook: a non-https URL, no events or an unknown one are refused; a good one is listed in words, then deleted", async ({
    page,
    request,
  }) => {
    const owner = await apiAs(request, cred("office (AFM)", "owner"))
    const url = `https://hooks.example.com/e2e/${unique()}`
    // An event the form cannot offer, sent straight to the API.
    await expect(
      owner.post(owner.ws("/webhook-endpoints"), { url, events: ["order.teleported"] })
    ).rejects.toThrow(/: 422 .*Unknown event order\.teleported\./)

    await loginOffice(page, cred("office (AFM)", "owner"))
    await page.goto(`${urls.office}/settings/webhooks`)
    await page.getByRole("button", { name: "New endpoint" }).click()
    const dialog = page.getByRole("dialog")
    const events = dialog.getByRole("group", { name: "Events" })
    await expectNoRawValues(page, { within: "[role=dialog]" })

    await dialog.getByLabel("URL").fill(url.replace("https:", "http:"))
    await events.getByRole("checkbox", { name: "Stock changed" }).check()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("URL")).toHaveAccessibleDescription("The url must be a public https URL.")

    await dialog.getByLabel("URL").fill(url)
    await events.getByRole("checkbox", { name: "Stock changed" }).uncheck()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(events).toHaveAccessibleDescription(/events/)

    await events.getByRole("checkbox", { name: "Stock changed" }).check()
    await events.getByRole("checkbox", { name: "Order placed" }).check()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByLabel("Secret")).toHaveText(/^whsec_/)
    await page.keyboard.press("Escape")
    await expect(page.getByText(/whsec_/)).toHaveCount(0)
    const row = page.getByRole("row").filter({ has: page.getByRole("cell", { name: url, exact: true }) })
    await expect(row).toContainText("Stock changed")
    await expect(row).toContainText("Order placed")
    await expectNoRawValues(page)

    const hooks = await owner.get<{ url: string; events: string[]; active: boolean }[]>(
      owner.ws("/webhook-endpoints?page_size=100")
    )
    const hook = hooks.find((h) => h.url === url)!
    expect(hook.active).toBe(true)
    // The API keeps events as a set; order is its own.
    expect([...hook.events].sort()).toEqual(["order.created", "stock.changed"])

    // Delete it again: the workspace allows at most 10 endpoints, and every run adds one.
    await row.getByRole("button", { name: "Delete" }).click()
    await page.getByRole("alertdialog").getByRole("button", { name: "Delete" }).click()
    await expect(page.getByRole("cell", { name: url, exact: true })).toHaveCount(0)
    const after = await owner.get<{ url: string }[]>(owner.ws("/webhook-endpoints?page_size=100"))
    expect(after.map((h) => h.url)).not.toContain(url)
  })
})
