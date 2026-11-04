import { tapKeys } from "./look"
import { expect, seedTill, test, WS } from "./fake-till"

const usersUrl = `/v1/workspace/${WS}/till/login/users`
const loginUrl = `/v1/workspace/${WS}/till/login`
const disconnected =
  "This till was disconnected or paired elsewhere. Pair it again with a new code from the office."

for (const { status, message } of [
  { status: 403, message: "This action is unauthorized." },
  { status: 404, message: "The requested resource could not be found." },
])
  test.describe(`the device key refused with ${status}`, () => {
    test.use({ allow: { responses: [{ url: usersUrl, status }] } })

    test("sends the till back to pair again", async ({ page, api }) => {
      api.on("GET", "/workspace/:ws/till/login/users", () => ({
        status,
        body: { message },
      }))
      await seedTill(page, { loggedIn: false })
      await page.goto("/login")
      await expect(
        page.getByRole("alert").filter({ hasText: disconnected })
      ).toBeVisible()
      await page.getByRole("button", { name: "Pair again" }).click()
      await expect(page).toHaveURL("/setup")
    })
  })

test.describe("a PIN the API finds malformed", () => {
  test.use({ allow: { responses: [{ url: loginUrl, status: 422 }] } })

  test("says what a PIN looks like", async ({ page, api }) => {
    api.on("POST", "/workspace/:ws/till/login", () => ({
      status: 422,
      body: {
        message: "The given data was invalid.",
        errors: { pin: ["The pin must be 4 to 8 digits."] },
      },
    }))
    await seedTill(page, { loggedIn: false })
    await page.goto("/login")
    await page.getByRole("button", { name: "Amina" }).click()
    await tapKeys(page, "1234")
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(
      page.getByRole("alert").filter({ hasText: "The PIN is 4 to 8 digits." })
    ).toBeVisible()
    await expect(page).toHaveURL("/login")
  })
})

test.describe("the key revoked after the names loaded", () => {
  test.use({ allow: { responses: [{ url: loginUrl, status: 401 }] } })

  // B1: the API refuses the device key (401 "You are not authorized…"), the till maps
  // every 401 to "Wrong PIN." and the cashier keeps guessing instead of pairing again.
  test.fail(
    "the PIN screen tells the cashier to pair again",
    async ({ page, api }) => {
      api.on("POST", "/workspace/:ws/till/login", () => ({
        status: 401,
        body: { message: "You are not authorized to perform this action." },
      }))
      await seedTill(page, { loggedIn: false })
      await page.goto("/login")
      await page.getByRole("button", { name: "Amina" }).click()
      await tapKeys(page, "1234")
      await page.getByRole("button", { name: "Log in" }).click()
      await expect(
        page.getByRole("alert").filter({ hasText: disconnected })
      ).toBeVisible()
      await expect(page.getByText("Wrong PIN.")).toHaveCount(0)
    }
  )
})
