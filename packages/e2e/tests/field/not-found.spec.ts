import type { Page } from "@playwright/test"

import { mockApi, type MockReply } from "../../src/api-mock"
import { seedSession } from "../../src/session"
import { expect, expectNoHorizontalScroll, test } from "../../src/test"
import { me, role, villages } from "./api"
import { farmerHome } from "./farmer.api"
import { crops, officerHome } from "./officer-records.api"

/**
 * A record the API answers 404 for (gone, or outside this user's villages or account) is an answer:
 * the screen says it is not found, in the shared EmptyState, never an error or an empty form.
 */
const ID = "90000000-0000-4000-8000-000000000009"
const W = "/workspace/w1/farm"
const gone: MockReply = { status: 404, body: { message: "The requested resource could not be found." } }

/** `also`: further reads of the same record, 404 too on the real API. */
type Case = { role: "farmer" | "field_officer" | "ops"; path: string; api: string; title: string; also?: string[] }

const cases: Case[] = [
  { role: "field_officer", path: `/officer/people/${ID}`, api: `${W}/persons/${ID}`, title: "Person not found" },
  { role: "field_officer", path: `/officer/cycles/${ID}`, api: `${W}/crop-cycles/${ID}`, title: "Crop cycle not found" },
  { role: "farmer", path: `/farm/surveys/${ID}`, api: `${W}/surveys/${ID}`, title: "This survey is not available." },
  { role: "farmer", path: `/farm/requests/${ID}`, api: `${W}/pue-requests/${ID}`, title: "Request not found" },
  { role: "ops", path: `/ops/requests/${ID}`, api: `${W}/pue-requests/${ID}`, title: "Request not found" },
  { role: "ops", path: `/ops/demand/${ID}`, api: `${W}/buyer-demands/${ID}`, title: "Demand not found", also: [`${W}/buyer-demands/${ID}/matches`] },
  { role: "ops", path: `/ops/opportunities/${ID}`, api: `${W}/opportunities/${ID}`, title: "Opportunity not found" },
]

async function open(page: Page, c: Case) {
  await seedSession(page)
  await page.addInitScript(() =>
    localStorage.setItem("ruaha360:tours-seen", JSON.stringify(["farmer:u1", "officer:u1", "ops:u1"])),
  )
  await mockApi(page, {
    ...farmerHome,
    ...officerHome,
    "GET /farm/me": me({ memberships: [role(c.role, c.role === "ops" ? null : { id: "v1", name: "Ilundo" })] }),
    [`GET ${W}/villages`]: villages,
    [`GET ${W}/crops`]: crops,
    ...Object.fromEntries([c.api, ...(c.also ?? [])].map((api) => [`GET ${api}`, gone])),
  })
  await page.goto(c.path)
}

for (const c of cases) {
  test.describe(`${c.path.replace(ID, "$id")} for a record the API does not show`, () => {
    test.use({ allow: { responses: [c.api, ...(c.also ?? [])].map((api) => ({ url: `/v1${api}`, status: 404 })) } })

    test("says not found, not an error", async ({ page }) => {
      await open(page, c)
      await expect(page.getByTestId("empty-state")).toContainText(c.title)
      await expect(page.getByTestId("error-state")).toHaveCount(0)
      await expectNoHorizontalScroll(page)
    })
  })
}

test("a path that is no screen says so and links home", async ({ page }) => {
  await open(page, { role: "field_officer", path: "/officer/nowhere", api: `${W}/unused`, title: "" })
  await expect(page.getByText("That page does not exist")).toBeVisible()
  await page.getByRole("link", { name: /home/i }).click()
  await expect(page).toHaveURL(/\/officer$/)
})
