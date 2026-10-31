import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import { PERSON, crops, issuedLogin, phoneRefused, registered } from "./register.api"

// The handset's GPS answers, so the farm location fills itself as it does in the field.
test.use({ geolocation: { latitude: -7.71, longitude: 35.71 }, permissions: ["geolocation"] })

async function open(page: Page, registration: MockReply) {
  await seedSession(page)
  // The first-visit guided tour would take the officer to /officer; this officer has seen it.
  await page.addInitScript(() => localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["officer:u1"])))
  await mockApi(page, {
    "GET /farm/me": me(),
    "GET /workspace/w1/farm/villages": villages,
    "GET /workspace/w1/farm/crops": crops,
    "POST /workspace/w1/farm/registrations": registration,
    [`POST /workspace/w1/farm/persons/${PERSON}/login`]: issuedLogin,
  })
  await page.goto("/officer/register")
  await expect(page).toHaveURL(/draft=/)
}

async function fillAndSubmit(page: Page) {
  await page.getByTestId("register-given-name").fill("Neema")
  await page.getByTestId("register-family-name").fill("Mwakalinga")
  await page.getByTestId("register-phone").fill("0712 345 678")
  await page.getByTestId("register-farm-label").fill("Riverside farm")
  await expect(page.getByTestId("register-farm-latitude")).toHaveValue(/-7\.71/)
  await page.getByTestId("register-plot-label").fill("Riverside")
  await page.getByRole("checkbox", { name: "Maize" }).check()
  await page.getByTestId("register-cycle-area").fill("1.6")
  await page.getByTestId("register-harvest-kg").fill("4100")
  await page.getByTestId("register-submit").click()
}

test("an officer registers a farmer and is shown the farmer's temporary password once", async ({ page }) => {
  await open(page, registered())
  // Only active crops are offered.
  await expect(page.getByRole("checkbox", { name: "Retired" })).toHaveCount(0)

  const sent = page.waitForRequest("**/v1/workspace/w1/farm/registrations")
  await fillAndSubmit(page)
  const body = (await sent).postDataJSON()
  expect(body.client_ref).toBe(new URL(page.url()).searchParams.get("draft"))
  expect(body).toMatchObject({
    village_id: "v1",
    person: { given_name: "Neema", family_name: "Mwakalinga", phone: "0712 345 678" },
    household: { label: null },
    cycles: [{ crop_id: expect.any(String), area_ha: "1.6", tree_count: null, harvest: { quantity_kg: "4100" } }],
  })

  await expect(page.getByTestId("register-success")).toBeVisible()
  await expect(page.getByTestId("farmer-login-phone")).toHaveText("+255712345678")
  await expect(page.getByTestId("farmer-login-password")).toHaveText("K7QXM2PA")
  await expectNoHorizontalScroll(page)

  // Shown once: Done clears it.
  await page.getByTestId("farmer-login-done").click()
  await expect(page.getByTestId("farmer-login-password")).toHaveCount(0)
})

test.describe("a refused registration", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/farm/registrations", status: 422 }] } })

  test("the API's phone refusal is shown on the phone field", async ({ page }) => {
    await open(page, phoneRefused)
    await fillAndSubmit(page)

    // The API's sentence, in the app's words for it (dbMessages names this one exactly).
    await expect(page.getByTestId("register-phone-error")).toContainText(
      "The phone number must be a Tanzanian mobile number, for example +255 712 345 678."
    )
    await expect(page.getByTestId("register-phone")).toHaveAttribute("aria-invalid", "true")
    await expect(page.getByTestId("register-success")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})

test("a replayed registration (same client_ref) still shows one success and one login", async ({ page }) => {
  await open(page, registered(true))
  let issued = 0
  page.on("request", (r) => {
    if (r.url().endsWith(`/persons/${PERSON}/login`)) issued++
  })
  await fillAndSubmit(page)

  await expect(page.getByTestId("register-success")).toHaveCount(1)
  await expect(page.getByTestId("farmer-login-card")).toHaveCount(1)
  await expect(page.getByTestId("farmer-login-password")).toHaveText("K7QXM2PA")
  expect(issued).toBe(1)
})
