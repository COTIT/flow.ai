import { expectNoHorizontalScroll } from "../../src/test"
import { expectTillLook, tapKeys } from "./look"
import {
  DEVICE_KEY,
  expect,
  seedTill,
  session,
  test,
  TOKEN,
  WS,
} from "./fake-till"

const loginUrl = `/v1/workspace/${WS}/till/login`
const pairUrl = "/v1/till/pair"
const usersUrl = `/v1/workspace/${WS}/till/login/users`

test("an unpaired till asks for its account and a pairing code", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page).toHaveURL("/setup")
  await expect(
    page.getByRole("heading", { name: "Pair this till" })
  ).toBeVisible()
  await expect(
    page.getByText("Valid for 10 minutes, works once.")
  ).toBeVisible()
  await expectTillLook(page)
})

test("pairing, picking a user and the right PIN open the sale screen", async ({
  page,
  api,
}) => {
  api.on("POST", "/till/pair", () => ({
    data: {
      workspace_id: WS,
      device_key: DEVICE_KEY,
      register: { id: "reg-1", name: "Iringa till 1" },
    },
  }))
  await page.goto("/setup")
  await page.getByLabel("Account").fill("afm")
  await page.getByLabel("Pairing code").fill("k7q4 mzpx")
  await page.getByRole("button", { name: "Pair this till" }).click()

  await expect(page).toHaveURL("/login")
  // The register's name (display only) labels the login bar, as office named it
  // (hidden on a phone, like the mockup's sale bar: no room).
  await expect(page.getByRole("banner")).toContainText("Iringa till 1")
  // Shop name and code go as typed; the API normalizes them.
  const [paired] = api.callsTo("POST", "/till/pair")
  expect(paired!.body).toEqual({ workspace: "afm", code: "k7q4 mzpx" })
  // Neither the workspace id nor the key ever shows on screen.
  await expect(page.getByText(WS, { exact: true })).toHaveCount(0)
  await expect(page.getByText(DEVICE_KEY)).toHaveCount(0)
  const stored = await page.evaluate(() => localStorage.getItem("genesy.till"))
  expect(JSON.parse(stored!).state.device).toEqual({
    workspaceId: WS,
    deviceKey: DEVICE_KEY,
    registerName: "Iringa till 1",
  })
  await expect(
    page.getByRole("heading", { name: "Who’s at the till?" })
  ).toBeVisible()
  await page.getByRole("button", { name: "Amina" }).click()
  await expect(page.getByRole("button", { name: "Amina" })).toHaveAttribute(
    "aria-pressed",
    "true"
  )
  await tapKeys(page, "1235")
  await page.getByRole("button", { name: "Delete" }).click()
  await tapKeys(page, "4")
  await expect(
    page.getByRole("img", { name: "4 of 4 digits entered" })
  ).toBeVisible()
  await expectTillLook(page)
  await page.getByRole("button", { name: "Log in" }).click()

  await expect(page).toHaveURL("/sale")
  await expect(page.getByText("Iringa till 1")).toBeVisible()
  await expect(page.getByText("Amina")).toBeVisible()

  // The login routes take the device key, never a till token.
  const [users] = api.callsTo("GET", "/workspace/w1/till/login/users")
  const [login] = api.callsTo("POST", "/workspace/w1/till/login")
  expect(users!.headers.authorization).toBe(`Bearer ${DEVICE_KEY}`)
  expect(login!.headers.authorization).toBe(`Bearer ${DEVICE_KEY}`)
  expect(login!.body).toEqual({ till_user_id: "tu-1", pin: "1234" })
})

test("logging out ends the session with the till token and returns to the PIN screen", async ({
  page,
  api,
}) => {
  await seedTill(page)
  await page.goto("/sale")
  await page.getByRole("button", { name: "Log out" }).click()

  await expect(page).toHaveURL("/login")
  const [logout] = api.callsTo("POST", "/workspace/w1/till/logout")
  expect(logout!.headers.authorization).toBe(`Bearer ${TOKEN}`)
  const saved = await page.evaluate(() => localStorage.getItem("genesy.till"))
  expect(JSON.parse(saved!).state.session).toBeNull()
  expect(saved).not.toContain(TOKEN)
})

test.describe("wrong PIN", () => {
  test.use({ allow: { responses: [{ url: loginUrl, status: 401 }] } })

  test("says so and stays on the PIN screen", async ({ page, api }) => {
    api.on("POST", "/workspace/:ws/till/login", () => ({
      status: 401,
      body: { message: "Invalid credentials" },
    }))
    await seedTill(page, { loggedIn: false })
    await page.goto("/login")
    await page.getByRole("button", { name: "Amina" }).click()
    await tapKeys(page, "9999")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "Wrong PIN." })
    ).toBeVisible()
    await expect(page).toHaveURL("/login")
    // The pad starts over for the next try.
    await expect(
      page.getByRole("img", { name: "0 of 4 digits entered" })
    ).toBeVisible()
    await expectTillLook(page)
  })
})

test.describe("locked user", () => {
  test.use({ allow: { responses: [{ url: loginUrl, status: 429 }] } })

  test("tells the cashier to wait", async ({ page, api }) => {
    api.on("POST", "/workspace/:ws/till/login", () => ({
      status: 429,
      body: { message: "Too Many Attempts." },
    }))
    await seedTill(page, { loggedIn: false })
    await page.goto("/login")
    await page.getByRole("button", { name: "Amina" }).click()
    await tapKeys(page, "1234")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Too many wrong PINs. Try again later." })
    ).toBeVisible()
  })
})

test.describe("revoked device key", () => {
  test.use({ allow: { responses: [{ url: usersUrl, status: 401 }] } })

  test("sends the till back to setup", async ({ page, api }) => {
    api.on("GET", "/workspace/:ws/till/login/users", () => ({
      status: 401,
      body: { message: "Unauthenticated." },
    }))
    await seedTill(page, { loggedIn: false })
    await page.goto("/login")
    await expect(
      page.getByRole("alert").filter({
        hasText:
          "This till was disconnected or paired elsewhere. Pair it again with a new code from the office.",
      })
    ).toBeVisible()
    await page.getByRole("button", { name: "Pair again" }).click()
    await expect(page).toHaveURL("/setup")
  })
})

test("a login stored before the register had a currency is refreshed from /till/me", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/till/shift", () => ({ data: null }))
  const { currency, accepted_cash, ...old } = session.register
  void currency
  void accepted_cash
  await page.addInitScript(
    (v) => localStorage.setItem("genesy.till", v),
    JSON.stringify({
      state: {
        device: { workspaceId: WS, deviceKey: DEVICE_KEY },
        session: { ...session, register: old },
      },
      version: 0,
    })
  )
  await page.goto("/drawer")
  await expect(page.getByLabel("Pieces of €20.00")).toBeVisible()
  const saved = await page.evaluate(() => localStorage.getItem("genesy.till"))
  expect(JSON.parse(saved!).state.session.register.currency).toEqual({
    code: "EUR",
    decimals: 2,
  })
})

test("a register with no till users says who can add one", async ({
  page,
  api,
}) => {
  api.on("GET", "/workspace/:ws/till/login/users", () => ({ data: [] }))
  await seedTill(page, { loggedIn: false })
  await page.goto("/login")
  await expect(
    page.getByText("No till users yet. Ask the office to add one.")
  ).toBeVisible()
  await expectTillLook(page)
})

test("the PIN pad waits for a name; a hardware keyboard types too; Not you? starts over", async ({
  page,
  api,
}) => {
  await seedTill(page, { loggedIn: false })
  await page.goto("/login")
  await expect(page.getByText("Tap your name first.")).toBeVisible()
  await expect(
    page.getByRole("button", { name: "1", exact: true })
  ).toBeDisabled()
  await expect(page.getByRole("button", { name: "Log in" })).toBeDisabled()
  await expectTillLook(page)

  await page.getByRole("button", { name: "Baraka" }).click()
  await page.keyboard.type("4321")
  await page.keyboard.press("Backspace")
  await expect(
    page.getByRole("img", { name: "3 of 4 digits entered" })
  ).toBeVisible()
  await page.getByRole("button", { name: "Clear" }).click()
  await expect(
    page.getByRole("img", { name: "0 of 4 digits entered" })
  ).toBeVisible()

  await page.getByRole("button", { name: "Not you?" }).click()
  await expect(page.getByText("Tap your name first.")).toBeVisible()
  await page.getByRole("button", { name: "Amina" }).click()
  await page.keyboard.type("123456")
  await expect(
    page.getByRole("img", { name: "6 of 6 digits entered" })
  ).toBeVisible()
  await page.keyboard.press("Enter")
  await expect(page).toHaveURL("/sale")
  const [login] = api.callsTo("POST", "/workspace/w1/till/login")
  expect(login!.body).toEqual({ till_user_id: "tu-1", pin: "123456" })
})

test.describe("till users unavailable", () => {
  test.use({
    allow: {
      responses: [{ url: `/v1/workspace/${WS}/till/login/users`, status: 503 }],
    },
  })

  test("says the list couldn't load", async ({ page, api }) => {
    api.on("GET", "/workspace/:ws/till/login/users", () => ({
      status: 503,
      body: { message: "Service unavailable." },
    }))
    await seedTill(page, { loggedIn: false })
    await page.goto("/login")
    await expect(
      page
        .getByRole("alert")
        .filter({ hasText: "Couldn't load the till users." })
    ).toBeVisible()
  })
})

test.describe("pairing problems", () => {
  for (const [status, text, role] of [
    [
      401,
      "The shop name and code don't match. Check both and try again.",
      "alert",
    ],
    [
      410,
      "This code has expired or was already used. Ask the office for a new one.",
      "status",
    ],
    [429, "Too many tries. Wait 15 minutes, then try again.", "alert"],
    [
      500,
      "Couldn't reach the server. Check the connection and try again.",
      "alert",
    ],
  ] as const) {
    test.describe(`${status}`, () => {
      test.use({ allow: { responses: [{ url: pairUrl, status }] } })

      test(`shows "${text}" and stays on setup`, async ({ page, api }) => {
        api.on("POST", "/till/pair", () => ({
          status,
          body: { message: "x" },
        }))
        await page.goto("/setup")
        await page.getByLabel("Account").fill("afm")
        await page.getByLabel("Pairing code").fill("K7Q4-MZPX")
        await page.getByRole("button", { name: "Pair this till" }).click()
        await expect(
          page.getByRole(role).filter({ hasText: text })
        ).toBeVisible()
        await expect(page).toHaveURL("/setup")
        await expectTillLook(page)
        const stored = await page.evaluate(() =>
          localStorage.getItem("genesy.till")
        )
        expect(stored ? JSON.parse(stored).state.device : null).toBeNull()
      })
    })
  }
})
