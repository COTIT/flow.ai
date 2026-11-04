import type { Page } from "@playwright/test"

import { test as guarded } from "../../src/test"
import {
  choose,
  expect,
  openMenu,
  fakeApi,
  me,
  paged,
  test,
  WS,
  WS2,
} from "./fake-api"

const refused = {
  status: 403,
  body: { message: "This action is unauthorized." },
}

/** A member of both workspaces with no team grants: the API refuses every admin call. */
const member = { ...me, permissions: [] }

const product = (id: string, name: string) => ({
  id,
  name,
  product_nr: id,
  reference: null,
  price_incl: "9.00",
  status: "active",
})

const settings = [
  { path: "/settings/app-keys", api: "app-keys" },
  { path: "/settings/webhooks", api: "webhook-endpoints" },
  { path: "/settings/invites", api: "invites" },
]

for (const { path, api: list } of settings)
  test.describe(`a member opens ${path}`, () => {
    test("is told they have no access; the list is never asked for, no rows", async ({
      page,
      api,
    }) => {
      api
        .on("GET", "/me", () => ({ data: { user: member } }))
        .on("GET", "/workspace/:ws/users/teams", () => paged([]))
        .on("GET", `/workspace/:ws/${list}`, () => refused)
      await page.goto(path)
      await expect(page.getByText("You don't have access to this")).toBeVisible()
      await expect(page.getByRole("row")).toHaveCount(0)
      expect(api.callsTo("GET", `/workspace/:ws/${list}`)).toHaveLength(0)
    })
  })

test.describe("a member's admin action", () => {
  test.use({
    allow: {
      responses: [
        { url: `/v1/workspace/${WS}/app-keys`, status: 403 },
        { url: `/v1/workspace/${WS}/app-keys/key-1`, status: 403 },
        { url: `/v1/workspace/${WS}/webhook-endpoints/wh-1`, status: 403 },
      ],
    },
  })

  test("creating an app key is refused in the dialog, no secret shown", async ({
    page,
    api,
  }) => {
    api
      .on("GET", "/workspace/:ws/app-keys", () => paged([]))
      .on("POST", "/workspace/:ws/app-keys", () => refused)
    await page.goto("/settings/app-keys")
    await page.getByRole("button", { name: "New app key" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("Name").fill("Till")
    await dialog.getByLabel("Kind").selectOption({ label: "Till" })
    await dialog.getByRole("checkbox", { name: "Read catalogue" }).check()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(dialog.getByText("This action is unauthorized.")).toBeVisible()
    await expect(dialog.getByLabel("Secret")).toHaveCount(0)
  })

  test("revoking an app key is refused and the key stays live", async ({
    page,
    api,
  }) => {
    api
      .on("GET", "/workspace/:ws/app-keys", () =>
        paged([
          {
            id: "key-1",
            name: "Shop",
            kind: "shop",
            scopes: ["catalog:read"],
            prefix: "gsk_ab12",
            created_at: "2026-10-01T08:00:00Z",
            last_used_at: null,
            revoked_at: null,
          },
        ])
      )
      .on("DELETE", "/workspace/:ws/app-keys/:key", () => refused)
    await page.goto("/settings/app-keys")
    await page.getByRole("button", { name: "Revoke" }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Revoke" })
      .click()
    await expect(page.getByText("This action is unauthorized.")).toBeVisible()
    await expect(page.getByText("Revoked", { exact: true })).toHaveCount(0)
  })

  test("deleting a webhook is refused and the endpoint stays", async ({
    page,
    api,
  }) => {
    api
      .on("GET", "/workspace/:ws/webhook-endpoints", () =>
        paged([
          {
            id: "wh-1",
            url: "https://erp.example.com/hooks",
            events: ["stock.changed"],
            description: null,
            active: true,
            created_at: "2026-10-01T08:00:00Z",
            updated_at: "2026-10-01T08:00:00Z",
          },
        ])
      )
      .on("DELETE", "/workspace/:ws/webhook-endpoints/:id", () => refused)
    await page.goto("/settings/webhooks")
    await page.getByRole("button", { name: "Delete" }).click()
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Delete" })
      .click()
    await expect(page.getByText("This action is unauthorized.")).toBeVisible()
    await expect(
      page.getByText("https://erp.example.com/hooks").first()
    ).toBeVisible()
  })
})

test.describe(() => {
  test.use({
    allow: {
      responses: [
        { url: `/v1/workspace/${WS}/webhook-endpoints`, status: 422 },
      ],
    },
  })

  test("a webhook with a bad URL shows the API's 422", async ({
    page,
    api,
  }) => {
    api
      .on("GET", "/workspace/:ws/webhook-endpoints", () => paged([]))
      .on("POST", "/workspace/:ws/webhook-endpoints", () => ({
        status: 422,
        body: {
          message: "The given data was invalid.",
          errors: { url: ["The url must be a valid https URL."] },
        },
      }))
    await page.goto("/settings/webhooks")
    await page.getByRole("button", { name: "New endpoint" }).click()
    const dialog = page.getByRole("dialog")
    await dialog.getByLabel("URL").fill("http://insecure.example.com")
    await dialog.getByRole("checkbox", { name: "Order placed" }).check()
    await dialog.getByRole("button", { name: "Save" }).click()
    await expect(
      dialog.getByText("The url must be a valid https URL.")
    ).toBeVisible()
    await expect(dialog.getByLabel("Secret")).toHaveCount(0)
  })
})

/** Storage, cookies and every console line, as one string to search for a leak. */
async function kept(page: Page, lines: string[]) {
  const stored = await page.evaluate(() =>
    JSON.stringify({ ...localStorage, ...sessionStorage, c: document.cookie })
  )
  return (
    stored + JSON.stringify(await page.context().cookies()) + lines.join("\n")
  )
}

test("new app key and webhook secrets are kept nowhere once shown", async ({
  page,
  api,
}) => {
  const KEY = "gsk_leakcheck_9f3a"
  const SECRET = "whsec_leakcheck_77b2"
  const lines: string[] = []
  page.on("console", (m) => lines.push(m.text()))
  const at = "2026-10-01T08:00:00Z"
  api
    .on("GET", "/workspace/:ws/app-keys", () => paged([]))
    .on("POST", "/workspace/:ws/app-keys", ({ body }) => ({
      status: 201,
      body: {
        data: {
          ...(body as object),
          id: "key-9",
          prefix: "gsk_leak",
          created_at: at,
          last_used_at: null,
          revoked_at: null,
          key: KEY,
        },
      },
    }))
    .on("GET", "/workspace/:ws/webhook-endpoints", () => paged([]))
    .on("POST", "/workspace/:ws/webhook-endpoints", ({ body }) => ({
      status: 201,
      body: {
        data: {
          ...(body as object),
          id: "wh-9",
          active: true,
          created_at: at,
          updated_at: at,
          secret: SECRET,
        },
      },
    }))

  await page.goto("/settings/app-keys")
  await page.getByRole("button", { name: "New app key" }).click()
  let dialog = page.getByRole("dialog")
  await dialog.getByLabel("Name").fill("Till")
  await dialog.getByLabel("Kind").selectOption({ label: "Till" })
  await dialog.getByRole("checkbox", { name: "Read catalogue" }).check()
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog.getByLabel("Secret")).toHaveText(KEY)
  await dialog.getByRole("button", { name: "I saved it" }).click()
  await expect(dialog).toBeHidden()

  await page.goto("/settings/webhooks")
  await page.getByRole("button", { name: "New endpoint" }).click()
  dialog = page.getByRole("dialog")
  await dialog.getByLabel("URL").fill("https://crm.example.com/in")
  await dialog.getByRole("checkbox", { name: "Order placed" }).check()
  await dialog.getByRole("button", { name: "Save" }).click()
  await expect(dialog.getByLabel("Secret")).toHaveText(SECRET)
  await page.keyboard.press("Escape")
  await expect(dialog).toBeHidden()

  const everywhere = await kept(page, lines)
  expect(everywhere.includes(KEY), "app key secret kept").toBe(false)
  expect(everywhere.includes(SECRET), "webhook secret kept").toBe(false)
  await expect(page.getByText(KEY)).toHaveCount(0)
  await expect(page.getByText(SECRET)).toHaveCount(0)
})

/** Products that belong to one workspace; the API answers per path workspace. */
function perWorkspace(api: Awaited<ReturnType<typeof fakeApi>>) {
  api.on("GET", "/workspace/:ws/products", ({ params }) =>
    paged(
      params[0] === WS
        ? [product("p1", "Honey of the first account")]
        : [product("p2", "Coffee of the second account")]
    )
  )
}

test("switching accounts never shows the other account's rows, there or back", async ({
  page,
  api,
}) => {
  perWorkspace(api)
  const first = page.getByText("Honey of the first account")
  const second = page.getByText("Coffee of the second account")
  await page.goto("/products")
  await expect(first).toBeVisible()
  await expect(second).toHaveCount(0)
  await openMenu(page)
  await choose(page, "Workspace", "Other Co")
  await expect(second).toBeVisible()
  await expect(first).toHaveCount(0)
  await openMenu(page)
  await choose(page, "Workspace", "Ilundo Honey Co.")
  await expect(first).toBeVisible()
  await expect(second).toHaveCount(0)
  expect(
    api.callsTo("GET", "/workspace/:ws/products").map((c) => c.path)
  ).toEqual(
    expect.arrayContaining([
      `/workspace/${WS}/products`,
      `/workspace/${WS2}/products`,
    ])
  )
})

guarded("the chosen account survives a reload", async ({ page }) => {
  // Seeded once, not on every load (seedSession would reset the choice on reload).
  await page.addInitScript(
    (v) => {
      if (!localStorage.getItem("genesy.session"))
        localStorage.setItem("genesy.session", v)
    },
    JSON.stringify({
      state: { token: "e2e-token", workspaceIds: [WS, WS2], workspaceId: WS },
      version: 0,
    })
  )
  perWorkspace(await fakeApi(page))
  await page.goto("/products")
  await openMenu(page)
  await choose(page, "Workspace", "Other Co")
  await expect(page.getByText("Coffee of the second account")).toBeVisible()
  await page.reload()
  await openMenu(page)
  await expect(page.getByRole("combobox", { name: "Workspace" })).toHaveText(
    /Other Co/
  )
  await expect(page.getByText("Coffee of the second account")).toBeVisible()
  await expect(page.getByText("Honey of the first account")).toHaveCount(0)
})

test("rights follow the account: owner of the first, read-only in the second", async ({
  page,
  api,
}) => {
  perWorkspace(api)
  api.on("GET", "/me", () => ({
    data: {
      user: {
        ...me,
        permissions: [
          { workspace: WS, permissions: ["workspace_owner"] },
          { workspace: WS2, permissions: ["product.list"] },
        ],
      },
    },
  }))
  await page.goto("/products")
  await expect(page.getByRole("button", { name: "New product" })).toBeEnabled()
  await openMenu(page)
  await choose(page, "Workspace", "Other Co")
  await expect(page.getByText("Coffee of the second account")).toBeVisible()
  // On a phone the menu stays open over the page; close it first.
  await page.keyboard.press("Escape")
  await expect(page.getByRole("button", { name: "New product" })).toBeDisabled()
})

const CODE = "InvAcc3ss7"
const invite = (new_user: boolean) => ({
  data: {
    id: "inv-1",
    email: "amina@example.com",
    invited_on: "2026-11-01T08:00:00Z",
    new_user,
    status: "pending",
    workspace_name: "ilundo-honey",
    display_name: "Ilundo Honey Co.",
  },
})

test.describe("an invite used up while the form was open", () => {
  test.use({ allow: { responses: [{ url: "/v1/register", status: 404 }] } })

  guarded("register says so and signs nobody in", async ({ page }) => {
    const api = await fakeApi(page)
    api
      .on("GET", "/invites/get-invite-status/:code", () => invite(true))
      .on("POST", "/register", () => ({
        status: 404,
        body: { message: "The requested resource could not be found." },
      }))
    await page.goto(`/accept-invitation?code=${CODE}`)
    await page.getByLabel("First name").fill("Amina")
    await page.getByLabel("Last name").fill("Mushi")
    await page.getByLabel("Password").fill("Honey!2026")
    await page.getByRole("button", { name: "Create account" }).click()
    await expect(
      page.getByText("The requested resource could not be found.")
    ).toBeVisible()
    await expect(page).toHaveURL(`/accept-invitation?code=${CODE}`)
    expect(
      await page.evaluate(
        () =>
          JSON.parse(localStorage.getItem("genesy.session") ?? "{}").state
            ?.token ?? null
      )
    ).toBeNull()
  })
})

for (const { status, message, why } of [
  {
    status: 403,
    message: "This invite is for another email address.",
    why: "an invite for someone else",
  },
  {
    status: 404,
    message: "The requested resource could not be found.",
    why: "an invite withdrawn meanwhile",
  },
])
  test.describe(why, () => {
    test.use({ allow: { responses: [{ url: "/v1/invites", status }] } })

    test("accepting shows the API's words and joins nothing", async ({
      page,
      api,
    }) => {
      api
        .on("GET", "/invites/get-invite-status/:code", () => invite(false))
        .on("POST", "/invites", () => ({ status, body: { message } }))
      await page.goto(`/accept-invitation?code=${CODE}`)
      await page.getByRole("button", { name: "Accept" }).click()
      await expect(page.getByText(message)).toBeVisible()
      await expect(page.getByText("You joined Ilundo Honey Co.")).toHaveCount(0)
    })
  })
