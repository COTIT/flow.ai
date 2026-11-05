import { amina, expect, register, seedTill, session, test, type FakeApi, WS } from "./fake-till"
import { expectTillLook, tapKeys } from "./look"

const openShift = {
  id: "sh-1",
  register_id: "reg-1",
  opened_by: "tu-1",
  opened_at: "2026-11-01T08:00:00Z",
  opening: 4515,
  expected: 6515,
  closed_by: null,
  closed_at: null,
  counted: null,
  difference: null,
}

type Line = { denomination?: number; value?: string; count: number }

function drawerApi(api: FakeApi, start: typeof openShift | null) {
  let shift = start
  api
    .on("GET", "/workspace/:ws/till/shift", () => ({ data: shift }))
    // The API's live figures; this fake adds up minor-unit lines and knows "0.50" only.
    .on("POST", "/workspace/:ws/till/shift/count-quote", ({ body }) => {
      const total = (body as { count: Line[] }).count.reduce(
        (sum, l) => sum + (l.denomination ?? (l.value === "0.50" ? 50 : 0)) * l.count,
        0
      )
      const expected = shift?.expected ?? null
      const lines = (body as { count: Line[] }).count.map(
        (l) => (l.denomination ?? (l.value === "0.50" ? 50 : 0)) * l.count
      )
      return {
        data: { total, lines, expected, difference: expected == null ? null : total - expected },
      }
    })
    .on("POST", "/workspace/:ws/till/shift/open", () => {
      shift = { ...openShift, expected: 4515 }
      return { status: 201, body: { data: shift } }
    })
    .on("POST", "/workspace/:ws/till/shift/close", () => {
      const closed = {
        ...shift!,
        closed_by: "tu-1",
        closed_at: "2026-11-01T17:00:00Z",
        counted: 6500,
        difference: -15,
      }
      shift = null
      return { data: closed }
    })
}

test("opening a shift sends the counted float and shows the API's figures", async ({
  page,
  api,
}) => {
  drawerApi(api, null)
  await seedTill(page)
  await page.goto("/sale")
  await page.getByRole("link", { name: "Drawer", exact: true }).click()
  await expect(
    page.getByText("No shift is open on this register.")
  ).toBeVisible()
  await expect(
    page.getByRole("heading", { name: "Count the float" })
  ).toBeVisible()
  await expectTillLook(page)

  // One row per note or coin the register accepts (from the login's register).
  await expect(page.getByLabel(/^Pieces of /)).toHaveCount(10)
  // Steppers, the pad (into the selected row) and a keyboard all count.
  await page.getByRole("button", { name: "One more €20.00" }).click()
  await page.getByRole("button", { name: "One more €20.00" }).click()
  await page.getByLabel("Pieces of €0.05").click()
  await tapKeys(page, "34")
  await page.getByRole("button", { name: "Delete" }).click()
  await page.getByLabel("Pieces of €1.00").fill("1")
  await page.getByRole("button", { name: "One less €1.00" }).click()
  await expect(page.getByLabel("Pieces of €20.00")).toHaveValue("2")
  await expect(page.getByLabel("Pieces of €0.05")).toHaveValue("3")
  // The total is the API's (count-quote), never added up on the till.
  await expect(page.getByLabel("Counted")).toHaveText("€40.15")
  const quotes = api.callsTo("POST", `/workspace/${WS}/till/shift/count-quote`)
  expect(quotes.at(-1)!.body).toEqual({
    count: [
      { denomination: 2000, count: 2 },
      { denomination: 100, count: 0 },
      { denomination: 5, count: 3 },
    ],
  })
  // Debounced: a burst of taps is not one call per tap.
  expect(quotes.length).toBeLessThan(7)
  await expectTillLook(page)
  await page.getByRole("button", { name: "Open shift" }).click()

  await expect(page.getByText("€45.15").first()).toBeVisible()
  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Shift opened. You can start selling." })
  ).toBeVisible()
  const [open] = api.callsTo("POST", `/workspace/${WS}/till/shift/open`)
  expect(open!.body).toEqual({
    count: [
      { denomination: 2000, count: 2 },
      { denomination: 100, count: 0 },
      { denomination: 5, count: 3 },
    ],
  })
})

test("closing a shift shows the variance the API returns", async ({
  page,
  api,
}) => {
  drawerApi(api, openShift)
  await seedTill(page)
  await page.goto("/drawer")
  await expect(page.getByLabel("Expected")).toHaveText("€65.15")
  await expect(page.getByText("Shift open", { exact: true })).toBeVisible()
  await expectTillLook(page)

  await page.getByLabel("Pieces of €50.00").fill("1")
  // Live: the API compares the count with the drawer while the cashier counts.
  await expect(page.getByLabel("Counted")).toHaveText("€50.00")
  await expect(page.getByLabel("Difference")).toHaveText("-€15.15")
  await page.getByRole("button", { name: "Close shift" }).click()

  await expect(
    page
      .getByRole("status")
      .filter({ hasText: "Shift closed — short by €0.15" })
  ).toBeVisible()
  await expect(page.getByLabel("Difference")).toHaveText("-€0.15")
  await expectTillLook(page, { shift: false })
  await page.getByRole("button", { name: "Open new shift" }).click()
  await expect(
    page.getByText("No shift is open on this register.")
  ).toBeVisible()
})

test("a row without a valid value is refused before sending", async ({
  page,
  api,
}) => {
  drawerApi(api, null)
  await seedTill(page)
  await page.goto("/drawer")
  await page.getByLabel("Pieces of €20.00").fill("two")
  await page.getByRole("button", { name: "Open shift" }).click()
  await expect(
    page.getByRole("alert").filter({
      hasText:
        "Each row needs a value (e.g. 5.00) and a whole number of pieces.",
    })
  ).toBeVisible()
  expect(api.callsTo("POST", `/workspace/${WS}/till/shift/open`)).toHaveLength(
    0
  )
})

test.describe("shift already open elsewhere", () => {
  test.use({
    allow: {
      responses: [{ url: `/v1/workspace/${WS}/till/shift/open`, status: 409 }],
    },
  })

  test("shows the API's message", async ({ page, api }) => {
    drawerApi(api, null)
    api.on("POST", "/workspace/:ws/till/shift/open", () => ({
      status: 409,
      body: { message: "A shift is already open on this register." },
    }))
    await seedTill(page)
    await page.goto("/drawer")
    await page.getByLabel("Pieces of €10.00").fill("1")
    await page.getByRole("button", { name: "Open shift" }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "A shift is already open on this register." })
    ).toBeVisible()
  })
})

for (const [difference, words] of [
  [0, "Shift closed — the drawer balances"],
  [120, "Shift closed — over by €1.20"],
] as const) {
  test(`closing with a difference of ${difference} says "${words}"`, async ({
    page,
    api,
  }) => {
    drawerApi(api, openShift)
    api.on("POST", "/workspace/:ws/till/shift/close", () => ({
      data: {
        ...openShift,
        closed_by: "tu-1",
        closed_at: "2026-11-01T17:00:00Z",
        counted: openShift.expected + difference,
        difference,
      },
    }))
    await seedTill(page)
    await page.goto("/drawer")
    await page.getByLabel("Pieces of €50.00").fill("1")
    await page.getByRole("button", { name: "Close shift" }).click()
    await expect(
      page.getByRole("status").filter({ hasText: words })
    ).toBeVisible()
  })
}

test("Lock till on the close result ends the session", async ({
  page,
  api,
}) => {
  drawerApi(api, openShift)
  await seedTill(page)
  await page.goto("/drawer")
  await page.getByLabel("Pieces of €50.00").fill("1")
  await page.getByRole("button", { name: "Close shift" }).click()
  await page.getByRole("button", { name: "Lock till" }).click()
  await expect(page).toHaveURL("/login")
})

test("a typed note goes to the API as text; the total is the API's", async ({ page, api }) => {
  drawerApi(api, null)
  // A register that lists no notes: the cashier types each one.
  api.on("GET", "/workspace/:ws/till/me", () => ({
    data: { expires_at: session.expires_at, register: { ...register, accepted_cash: [] }, user: amina },
  }))
  await seedTill(page, { accepted: [] })
  await page.goto("/drawer")
  await page.getByLabel("Value of row 1").fill("0.50")
  await page.getByLabel("Pieces of row 1").fill("3")
  await expect(page.getByLabel("Counted")).toHaveText("€1.50")
  // The row's amount is the API's too.
  await expect(page.getByText("€1.50", { exact: true })).toHaveCount(2)
  expect(api.callsTo("POST", `/workspace/${WS}/till/shift/count-quote`).at(-1)!.body).toEqual({
    count: [{ value: "0.50", count: 3 }],
  })
  await page.getByRole("button", { name: "Open shift" }).click()
  await expect(
    page.getByRole("status").filter({ hasText: "Shift opened. You can start selling." })
  ).toBeVisible()
  const [open] = api.callsTo("POST", `/workspace/${WS}/till/shift/open`)
  expect(open!.body).toEqual({ count: [{ value: "0.50", count: 3 }] })
})

test("offline: no total on the till, it says the total shows when online", async ({ page, api, context }) => {
  drawerApi(api, null)
  await seedTill(page)
  await page.goto("/drawer")
  await expect(page.getByRole("heading", { name: "Count the float" })).toBeVisible()
  await context.setOffline(true)
  await page.getByLabel("Pieces of €20.00").fill("2")
  await expect(page.getByText("Total shown when online.")).toBeVisible()
  await expect(page.getByLabel("Counted")).toHaveText("—")
  await context.setOffline(false)
  await expect(page.getByLabel("Counted")).toHaveText("€40.00")
  await expect(page.getByText("Total shown when online.")).toHaveCount(0)
})
