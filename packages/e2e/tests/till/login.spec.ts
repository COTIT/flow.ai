import { expectNoHorizontalScroll } from "../../src/test"
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
const usersUrl = `/v1/workspace/${WS}/till/login/users`

test("an unpaired till asks for its workspace and device key", async ({
  page,
}) => {
  await page.goto("/")
  await expect(page).toHaveURL("/setup")
  await expect(
    page.getByRole("heading", { name: "Set up this till" })
  ).toBeVisible()
  await expectNoHorizontalScroll(page)
})

test("pairing, picking a user and the right PIN open the sale screen", async ({
  page,
  api,
}) => {
  await page.goto("/setup")
  await page.getByLabel("Workspace id").fill(WS)
  await page.getByLabel("Device key").fill(DEVICE_KEY)
  await page.getByRole("button", { name: "Save" }).click()

  await expect(page).toHaveURL("/login")
  await page.getByRole("button", { name: "Amina" }).click()
  await page.getByLabel("PIN").fill("1234")
  await expectNoHorizontalScroll(page)
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
    await page.getByLabel("PIN").fill("9999")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "Wrong PIN." })
    ).toBeVisible()
    await expect(page).toHaveURL("/login")
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
    await page.getByLabel("PIN").fill("1234")
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
          "This device's key is no longer accepted. Set up the till again.",
      })
    ).toBeVisible()
    await page.getByRole("button", { name: "Set up again" }).click()
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
