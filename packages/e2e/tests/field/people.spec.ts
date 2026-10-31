import { mockApi } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, villages } from "./api"
import { page as listPage, person, record, spouse, tourSeen } from "./officer-records.api"

test("an officer lists their people, filters by verification on the API and searches by name", async ({ page }) => {
  await seedSession(page)
  await tourSeen(page)
  await mockApi(page, { "GET /farm/me": me(), "GET /workspace/w1/farm/villages": villages })
  let filtered = false
  const reads = await record(page, "GET", "/workspace/w1/farm/persons", () => ({
    body: listPage(filtered ? [person()] : [person(), spouse]),
  }))
  await page.goto("/officer/people")

  await expect(page.getByTestId("people-row")).toHaveCount(2)
  await expect(page.getByTestId("people-row").first()).toContainText("Juma Mwakalinga")
  expect(reads[0]?.searchParams.get("verification")).toBeNull()

  filtered = true
  await page.getByTestId("people-filter-verification").click()
  await page.getByRole("option", { name: "Unverified" }).click()
  await expect(page.getByTestId("people-row")).toHaveCount(1)
  expect(reads.at(-1)?.searchParams.get("verification")).toBe("unverified")
  await expect(page).toHaveURL(/verification=unverified/)

  // shortcut: names are matched in the app until GET /farm/persons takes ?q=; the filter still goes to the API.
  filtered = false
  await page.goto("/officer/people?q=juma")
  await expect(page.getByTestId("people-row")).toHaveCount(1)
  await expect(page.getByTestId("people-row")).toContainText("Juma Mwakalinga")
  await expectNoHorizontalScroll(page)
})

test.describe("a refused list", () => {
  test.use({ allow: { responses: [{ url: "/v1/workspace/w1/farm/persons?page=1&page_size=100", status: 503 }] } })

  test("shows the API's message, not an empty list", async ({ page }) => {
    await seedSession(page)
    await tourSeen(page)
    await mockApi(page, {
      "GET /farm/me": me(),
      "GET /workspace/w1/farm/villages": villages,
      "GET /workspace/w1/farm/persons": { status: 503, body: { message: "the records service is busy" } },
    })
    await page.goto("/officer/people")
    await expect(page.getByTestId("error-state")).toContainText("the records service is busy", { timeout: 15_000 })
    await expect(page.getByTestId("people-row")).toHaveCount(0)
    await expectNoHorizontalScroll(page)
  })
})
