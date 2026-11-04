import { expect, paged, test, type FakeApi } from "./fake-api"

function keysApi(api: FakeApi) {
  const keys = [
    {
      id: "key-1",
      name: "Shop",
      kind: "shop",
      scopes: ["catalog:read"],
      prefix: "gk_live_ab12",
      created_at: "2026-10-01T08:00:00Z",
      last_used_at: null,
      revoked_at: null as string | null,
    },
  ]
  api
    .on("GET", "/workspace/:ws/app-keys", () => paged(keys))
    .on("POST", "/workspace/:ws/app-keys", ({ body }) => {
      const b = body as { name: string; kind: string; scopes: string[] }
      if (b.scopes.some((s) => s !== "catalog:read"))
        return {
          status: 422,
          body: {
            message: "The given data was invalid.",
            errors: { scopes: ["Unknown scope x."] },
          },
        }
      const key = { ...keys[0]!, ...b, id: "key-2", prefix: "gk_live_cd34" }
      keys.push(key)
      return {
        status: 201,
        body: { data: { ...key, key: "gk_live_cd34_SECRET" } },
      }
    })
    .on("DELETE", "/workspace/:ws/app-keys/:key", ({ params }) => {
      keys.find((k) => k.id === params[1])!.revoked_at = "2026-10-31T10:00:00Z"
      return { status: 204 }
    })
}

function hooksApi(api: FakeApi) {
  const hooks = [
    {
      id: "wh-1",
      url: "https://erp.example.com/hooks",
      events: ["stock.changed"],
      description: null,
      active: true,
      created_at: "2026-10-01T08:00:00Z",
      updated_at: "2026-10-01T08:00:00Z",
    },
  ]
  api
    .on("GET", "/workspace/:ws/webhook-endpoints", () => paged(hooks))
    .on("POST", "/workspace/:ws/webhook-endpoints", ({ body }) => {
      const hook = {
        ...hooks[0]!,
        ...(body as Record<string, unknown>),
        id: "wh-2",
      }
      hooks.push(hook)
      return {
        status: 201,
        body: { data: { ...hook, secret: "whsec_SECRET" } },
      }
    })
    .on("PATCH", "/workspace/:ws/webhook-endpoints/:id", ({ params, body }) => {
      const hook = hooks.find((h) => h.id === params[1])!
      Object.assign(hook, body)
      return { data: hook }
    })
    .on("DELETE", "/workspace/:ws/webhook-endpoints/:id", ({ params }) => {
      hooks.splice(
        hooks.findIndex((h) => h.id === params[1]),
        1
      )
      return { status: 204 }
    })
}

test("app key: secret shown once, gone after closing", async ({
  page,
  api,
}) => {
  keysApi(api)
  await page.goto("/settings/app-keys")
  await page.getByRole("button", { name: "New app key" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill("Counter")
  await dialog.getByLabel("Kind").selectOption({ label: "Till" })
  await dialog.getByRole("checkbox", { name: "Read catalogue" }).check()
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog.getByLabel("Secret")).toHaveText("gk_live_cd34_SECRET")
  expect(api.callsTo("POST", "/workspace/:ws/app-keys")[0]?.body).toEqual({
    name: "Counter",
    kind: "till",
    scopes: ["catalog:read"],
  })
  await expect(dialog.getByRole("button", { name: "Copy" })).toBeVisible()
  await dialog.getByRole("button", { name: "I saved it" }).click()
  await expect(dialog).toBeHidden()
  await expect(
    page.getByRole("cell", { name: "Counter", exact: true })
  ).toBeVisible()
  await expect(page.getByText("gk_live_cd34_SECRET")).toHaveCount(0)
  await page.getByRole("button", { name: "New app key" }).click()
  await expect(page.getByRole("dialog").getByLabel("Name")).toBeVisible()
  await expect(page.getByText("gk_live_cd34_SECRET")).toHaveCount(0)
})

test.describe(() => {
  test.use({
    allow: { responses: [{ url: "/v1/workspace/w1/app-keys", status: 422 }] },
  })

  test("app key: the API's 422 is shown", async ({ page, api }) => {
    keysApi(api)
    await page.goto("/settings/app-keys")
    await page.getByRole("button", { name: "New app key" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("Bad")
    await dialog.getByLabel("Kind").selectOption({ label: "Web shop" })
    await dialog.getByRole("checkbox", { name: "Take web orders" }).check()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByText("Unknown scope x.")).toBeVisible()
  })
})

test("revoke: cancelling sends nothing; confirming revokes", async ({
  page,
  api,
}) => {
  keysApi(api)
  await page.goto("/settings/app-keys")
  await page.getByRole("button", { name: "Revoke" }).click()
  await expect(page.getByRole("alertdialog")).toContainText("Revoke Shop?")
  await page.getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(api.callsTo("DELETE", "/workspace/:ws/app-keys/:key")).toHaveLength(0)

  await page.getByRole("button", { name: "Revoke" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Revoke" })
    .click()
  await expect(page.getByText("Revoked", { exact: true })).toBeVisible()
  await expect(page.getByRole("button", { name: "Revoke" })).toHaveCount(0)
  await expect(
    page.getByRole("status").filter({ hasText: "App key revoked." })
  ).toBeVisible()
  expect(api.callsTo("DELETE", "/workspace/:ws/app-keys/:key")).toHaveLength(1)
})

test("webhook: create shows secret once; edit pauses it", async ({
  page,
  api,
}) => {
  hooksApi(api)
  await page.goto("/settings/webhooks")
  await page.getByRole("button", { name: "New endpoint" }).click()
  const dialog = page.getByRole("dialog")
  await dialog.getByLabel("URL").fill("https://crm.example.com/in")
  await dialog.getByRole("checkbox", { name: "Order placed" }).check()
  await dialog.getByRole("checkbox", { name: "Passport published" }).check()
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog.getByLabel("Secret")).toHaveText("whsec_SECRET")
  expect(
    api.callsTo("POST", "/workspace/:ws/webhook-endpoints")[0]?.body
  ).toEqual({
    url: "https://crm.example.com/in",
    events: ["order.created", "passport.published"],
    description: null,
  })
  await page.keyboard.press("Escape")
  await expect(page.getByText("whsec_SECRET")).toHaveCount(0)

  await page
    .getByRole("button", { name: "Edit: https://erp.example.com/hooks" })
    .click()
  await page.getByRole("dialog").getByLabel("Active").uncheck()
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click()
  await expect(page.getByText("Paused")).toBeVisible()
  expect(
    api.callsTo("PATCH", "/workspace/:ws/webhook-endpoints/:id")[0]?.body
  ).toMatchObject({
    active: false,
  })
})

test("webhook delete: cancelling sends nothing; confirming deletes", async ({
  page,
  api,
}) => {
  hooksApi(api)
  await page.goto("/settings/webhooks")
  await page.getByRole("button", { name: "Delete" }).click()
  await page.getByRole("button", { name: "Cancel" }).click()
  await expect(page.getByRole("alertdialog")).toBeHidden()
  expect(
    api.callsTo("DELETE", "/workspace/:ws/webhook-endpoints/:id")
  ).toHaveLength(0)

  await page.getByRole("button", { name: "Delete" }).click()
  await page
    .getByRole("alertdialog")
    .getByRole("button", { name: "Delete" })
    .click()
  await expect(page.getByText("No webhook endpoints yet.")).toBeVisible()
})
