// Privileged Till returns, refunds and drawer adjustments (Flow ERP M7-D, masterplan §15 /
// TILL AUTHORIZATION): a member approves with their own login (memory only), the till finds the
// sale by receipt number, returns lines and refunds the credit in cash or as a recorded card
// refund; the drawer is adjusted with a count. Own active-book workspace per run (data rule 1).
import type { APIRequestContext, Page } from "@playwright/test"

import { activeBook, freshWorkspace, mainCurrency } from "../src/accounting"
import { psql, saleTaxProfile } from "../src/erp"
import {
  apiAs,
  cred,
  expect,
  mailTo,
  stackAllow,
  test,
  unique,
  urls,
  type Api,
} from "../src/stack"
import { expectNoHorizontalScroll } from "../src/test"
import { tillMoney, type Sale } from "./till"

type Register = { id: string; warehouse_id: string; currency_id: string }

const PIN = "4821"

/**
 * An active book with "Honey jar" (12.52 incl. 0 % tax, 10 on stock), 5-cent cash rounding, a
 * register, a till user and a clerk member who may only list registers.
 */
async function world(request: APIRequestContext) {
  const w = await freshWorkspace(request, "till-ret")
  const api = w.api
  await api.post(api.ws("/warehouses"), {
    name: "Main store",
    default_warehouse: true,
    is_store: true,
  })
  const store = (
    await api.get<{ id: string; name: string }[]>(
      api.ws("/warehouses?page_size=100")
    )
  ).find((x) => x.name === "Main store")!
  const product = `Honey jar ${unique()}`
  const { product_id } = await api.post<{ product_id: string }>(
    api.ws("/products"),
    {
      name: product,
      price: "12.52",
      has_stock: true,
      store_only: false,
    }
  )
  await api.post(api.ws(`/products/${product_id}/stocks`), {
    warehouse_id: store.id,
    mutation_value: "10",
    reason: "Opening",
  })
  await activeBook(request, api)
  // shortcut: the opening value activation will seed (M8 route), as src/erp.ts draftCredit.
  psql(`INSERT INTO inventory_cost_pools (workspace_id, product_id, warehouse_id, quantity, value)
        VALUES ('${api.workspace}'::uuid, '${product_id}'::uuid, '${store.id}'::uuid, 10, 10000)`)
  await saleTaxProfile(request, api)
  const currency = await mainCurrency(api)
  await api.post(api.ws("/settings"), {
    cash_register: {
      cash_rounding: [{ currency: currency.asset_type.toUpperCase(), step: 5 }],
    },
  })
  const cashier = `Cashier ${unique()}`
  const till = await api.post<{ id: string }>(api.ws("/till/users"), {
    name: cashier,
    pin: PIN,
  })
  const clerk = await clerkOf(request, api)
  const register = await api.post<Register>(api.ws("/till/registers"), {
    name: `e2e till ${unique()}`,
    warehouse_id: store.id,
    currency_id: currency.id,
  })
  const sale = await sellByApi(request, api, register, {
    till: till.id,
    product: product_id,
  })
  return {
    register,
    sale,
    ...w,
    product,
    store: store.id,
    currency: currency.id,
    cashier,
    clerk,
  }
}

/** A member with only `cashregister.list`, joined through a real invite. */
async function clerkOf(request: APIRequestContext, api: Api) {
  const { team_id } = await api.post<{ team_id: string }>(
    api.ws("/users/teams"),
    {
      name: `Clerks ${unique()}`,
      description: "e2e clerks",
      permissions: ["cashregister.list"],
    }
  )
  const email = `clerk-${unique()}@e2e.test`
  const password = `E2e!${unique()}Aa1`
  await api.post(api.ws("/invites"), { email, team_id })
  const code = (await mailTo(email)).match(/code=([0-9a-f]{64})/)?.[1]
  expect(code, "invite code").toBeTruthy()
  const r = await request.post(`${urls.api}/v1/register`, {
    data: {
      first_name: "Cleo",
      last_name: "Clerk",
      email,
      password,
      invite_code: code,
    },
  })
  expect(r.status(), "clerk register").toBe(201)
  return { email, password }
}

/**
 * The register paired through the API, its shift opened and 3 honey sold for cash (37.56, cash
 * due 37.55): the sale exists before the tests, so their allowed refusals can name its URL.
 */
async function sellByApi(
  request: APIRequestContext,
  api: Api,
  register: Register,
  ids: { till: string; product: string }
) {
  const call = async <T>(path: string, auth: string, data: unknown) => {
    const r = await request.post(`${urls.api}/v1${path}`, {
      headers: { Authorization: `Bearer ${auth}` },
      data,
    })
    expect(r.status(), path).toBeLessThan(300)
    return ((await r.json()) as { data: T }).data
  }
  const pairing = await api.post<{ code: string; workspace: string }>(
    api.ws(`/till/registers/${register.id}/pairing`)
  )
  const { device_key } = await call<{ device_key: string }>(
    "/till/pair",
    "",
    pairing
  )
  const { token } = await call<{ token: string }>(
    api.ws("/till/login"),
    device_key,
    { till_user_id: ids.till, pin: PIN }
  )
  await call(api.ws("/till/shift/open"), token, { count: [] })
  return call<Sale>(api.ws("/till/sales"), token, {
    client_ref: crypto.randomUUID(),
    lines: [{ product_id: ids.product, quantity: "3", unit_price: 1252 }],
    payment: { method: "cash", tendered: 4000 },
  })
}

type World = Awaited<ReturnType<typeof world>>

/**
 * `register` (default: a new one) paired through /setup, the till user logged in by PIN. Pairing
 * again rotates the register's key; its open shift stays.
 */
async function pair(page: Page, w: World, existing?: Register) {
  const api = w.api
  const register =
    existing ??
    (await api.post<Register>(api.ws("/till/registers"), {
      name: `e2e till ${unique()}`,
      warehouse_id: w.store,
      currency_id: w.currency,
    }))
  const pairing = await api.post<{ code: string; workspace: string }>(
    api.ws(`/till/registers/${register.id}/pairing`)
  )
  await page.goto(`${urls.till}/setup`)
  await page.getByLabel("Account").fill(pairing.workspace)
  await page.getByLabel("Pairing code").fill(pairing.code)
  await page.getByRole("button", { name: "Pair this till" }).click()
  await expect(page).toHaveURL(`${urls.till}/login`)
  await page.getByRole("button", { name: w.cashier }).click()
  for (const digit of PIN)
    await page.getByRole("button", { name: digit, exact: true }).click()
  await page.getByRole("button", { name: "Log in" }).click()
  await expect(page).not.toHaveURL(`${urls.till}/login`)
  return register
}

async function approve(page: Page, who: { login: string; secret: string }) {
  await page.getByLabel("Email").fill(who.login)
  await page.getByLabel("Password").fill(who.secret)
  await page.getByRole("button", { name: "Approve" }).click()
}

async function find(page: Page, number: string) {
  await page.getByLabel("Find a sale").fill(number)
  await page.getByRole("button", { name: "Find" }).click()
}

test.describe("till returns and refunds", () => {
  test.describe.configure({ mode: "serial" })
  let w: World
  test.beforeAll(async ({ request }) => {
    test.setTimeout(180_000)
    w = await world(request)
  })
  // The beforeAll request context can't serve tests: a fresh API client per test.
  test.beforeEach(async ({ request }) => {
    w.api = await apiAs(request, w.login)
  })
  const ws = () => `${urls.till}/v1/workspace/${w.api.workspace}/till`
  test.use({
    // eslint-disable-next-line no-empty-pattern -- Playwright reads fixture deps from the pattern
    allow: async ({}, use) =>
      use({
        ...stackAllow,
        responses: [
          ...stackAllow.responses,
          // The refusals under test; each is shown in words.
          { url: `${urls.till}/v1/login`, status: 401 },
          { url: `${ws()}/privileged-session`, status: 403 },
          { url: `${ws()}/privileged-session`, status: 404 },
          { url: `${ws()}/sales/lookup?number=no-such-receipt`, status: 404 },
          {
            url: `${ws()}/sales/${w.sale.order_id}/refunds/review`,
            status: 422,
          },
        ],
      }),
  })

  test("a manager approves, the till returns two of three and refunds cash in whole steps", async ({
    page,
  }) => {
    await pair(page, w, w.register)
    const money = await tillMoney(page)

    // The top bar has a Returns tab beside Lock on wider screens; phones go through the Drawer.
    if (page.viewportSize()!.width > 760)
      await page.getByRole("link", { name: "Returns", exact: true }).click()
    else {
      await page.getByRole("link", { name: "Drawer", exact: true }).click()
      await page.getByRole("link", { name: "Returns and refunds" }).click()
    }
    await approve(page, w.login)
    await expect(
      page.getByRole("status").filter({ hasText: /^Approved by / })
    ).toBeVisible()
    await find(page, w.sale.number)
    await expect(
      page.getByRole("heading", { name: `Sale ${w.sale.number}` })
    ).toBeVisible()
    await expect(page.getByText("Sold 3.00 · can return 3.00")).toBeVisible()
    await expectNoHorizontalScroll(page)

    const more = page.getByRole("button", {
      name: `Return one more ${w.product}`,
    })
    await more.click()
    await more.click()
    await page.getByLabel("Reason").first().fill("Cracked jars")
    await page.getByRole("button", { name: "Check return" }).click()
    await expect(
      page.getByRole("region", { name: "Check before booking" })
    ).toBeVisible()
    await page.getByRole("button", { name: "Book return" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: "Return booked." })
    ).toBeVisible()
    await expect(page.getByText("Sold 3.00 · can return 1.00")).toBeVisible()

    // All of the credit in cash breaks the 5-cent step: refused in words.
    const refund = page.getByRole("form", { name: "Refund" })
    const left = refund.getByLabel("Left to refund")
    await expect(left).not.toHaveText(money(0))
    await refund.getByRole("button", { name: "All of it" }).click()
    await refund.getByLabel("Reason").fill("Customer brought them back")
    await refund.getByRole("button", { name: /^Check refund/ }).click()
    await expect(refund).toContainText(
      "Cash refunds go in whole cash steps. Pay the rest by card."
    )

    // 25.00 in cash: the review says what goes out and what the drawer holds.
    await refund.getByRole("button", { name: "Clear" }).click()
    for (const d of "2500")
      await refund.getByRole("button", { name: d, exact: true }).click()
    await refund.getByRole("button", { name: /^Check refund/ }).click()
    await expect(
      refund.getByRole("region", { name: "Check before booking" })
    ).toContainText(money(2500))
    await refund.getByRole("button", { name: /^Book refund/ }).click()
    await expect(
      page
        .getByRole("status")
        .filter({ hasText: `Refund of ${money(2500)} booked.` })
    ).toBeVisible()

    expect(
      psql(
        `SELECT method || ':' || amount FROM till_refunds WHERE order_id = '${w.sale.order_id}'::uuid`
      )
    ).toBe("cash:2500")

    // The approval never touched storage: none of it in localStorage or cookies, gone on reload.
    const stored = await page.evaluate(
      () => JSON.stringify({ ...localStorage }) + document.cookie
    )
    expect(stored).not.toContain(w.login.login)
    await page.reload()
    await expect(page.getByRole("button", { name: "Approve" })).toBeVisible()
  })

  test("on another register only a card refund, with the terminal's reference and confirmation", async ({
    page,
  }) => {
    await pair(page, w)
    await page.goto(`${urls.till}/returns`)
    await approve(page, w.login)
    await find(page, w.sale.number)
    const refund = page.getByRole("form", { name: "Refund" })
    await expect(refund.getByRole("button", { name: "Cash" })).toBeDisabled()
    await expect(
      refund.getByText(/^Cash goes back only from the drawer of /)
    ).toBeVisible()
    await refund.getByRole("button", { name: "All of it" }).click()
    await refund.getByLabel("Reason").fill("Rest of the credit")
    await refund.getByRole("button", { name: /^Check refund/ }).click()
    await expect(refund).toContainText("Enter the terminal's refund reference.")
    await expect(refund).toContainText(
      "Record a card refund only after the terminal confirmed it."
    )
    await refund.getByLabel("Terminal refund reference").fill("TRM-42")
    await refund
      .getByRole("checkbox", { name: "The terminal confirmed the refund" })
      .check()
    await refund.getByRole("button", { name: /^Check refund/ }).click()
    await refund.getByRole("button", { name: /^Book refund/ }).click()
    await expect(
      page.getByRole("status").filter({ hasText: /^Refund of .+ booked\./ })
    ).toBeVisible()
    await expect(page.getByText("fully refunded")).toBeVisible()
    await expectNoHorizontalScroll(page)
  })

  test("a wrong login, a member without the right and an outsider are refused in words", async ({
    page,
  }) => {
    await pair(page, w)
    await page.goto(`${urls.till}/returns`)
    await approve(page, { login: w.login.login, secret: "not-the-password" })
    await expect(page.getByText("Email or password is wrong.")).toBeVisible()
    await approve(page, { login: w.clerk.email, secret: w.clerk.password })
    await expect(
      page.getByText("This person may not approve this. Ask a manager who may.")
    ).toBeVisible()
    await approve(page, cred("office (AFM)", "owner"))
    await expect(
      page.getByText("This person isn't part of this till's account.")
    ).toBeVisible()
    await approve(page, w.login)
    await find(page, "no-such-receipt")
    await expect(
      page.getByText("No till sale with number no-such-receipt.")
    ).toBeVisible()
    // Lock ends the approval with the cashier's session.
    await page.getByRole("button", { name: "Lock" }).click()
    await expect(page).toHaveURL(`${urls.till}/login`)
  })

  test("offline, approval waits for a connection and nothing is queued", async ({
    page,
    context,
  }) => {
    await pair(page, w)
    await page.goto(`${urls.till}/returns`)
    await expect(page.getByRole("button", { name: "Approve" })).toBeVisible()
    await context.setOffline(true)
    await expect(page.getByText("Needs a connection")).toBeVisible()
    await expect(page.getByRole("button", { name: "Approve" })).toHaveCount(0)
    await context.setOffline(false)
    await expect(page.getByRole("button", { name: "Approve" })).toBeVisible()
  })

  test("a manager adjusts the drawer cash to a count", async ({ page }) => {
    await pair(page, w, w.register)
    await page.goto(`${urls.till}/drawer`)
    await page.getByRole("link", { name: "Adjust drawer cash" }).click()
    await approve(page, w.login)
    await page
      .getByRole("button", { name: /^One more/ })
      .first()
      .click()
    await page.getByLabel("Count sheet or note").fill("Sheet 7")
    await page.getByLabel("Reason").fill("Counted at noon")
    await page.getByRole("button", { name: "Check adjustment" }).click()
    const review = page.getByRole("region", { name: "Check before booking" })
    await expect(review.getByLabel("Difference")).toBeVisible()
    await page.getByRole("button", { name: "Book adjustment" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: /^Adjustment of .+ booked\./ })
    ).toBeVisible()
    await expectNoHorizontalScroll(page)
  })
})
