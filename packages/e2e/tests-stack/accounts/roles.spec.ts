import type { APIRequestContext } from "@playwright/test"

import {
  apiAs,
  cred,
  expect,
  loginField,
  loginOffice,
  test,
  urls,
  workspaces,
} from "../../src/stack"
import { test as refusing } from "./refusals"

/** The API's status for `path` as one member, without the throw `apiAs` does on >= 400. */
async function statusAs(
  request: APIRequestContext,
  token: string,
  path: string,
  method = "GET"
) {
  const r = await request.fetch(`${urls.api}/v1${path}`, {
    method,
    headers: { Authorization: `Bearer ${token}` },
    data: method === "POST" ? {} : undefined,
  })
  return r.status()
}

/** A farm login's token from `POST /v1/farm/login` (phone) or `/v1/login` (email). */
async function tokenOf(
  request: APIRequestContext,
  who: { login: string; secret: string }
) {
  const phone = !who.login.includes("@")
  const r = await request.post(
    `${urls.api}/v1/${phone ? "farm/login" : "login"}`,
    {
      data: phone
        ? { phone: who.login, password: who.secret }
        : { email: who.login, password: who.secret },
    }
  )
  expect(r.status(), `login ${phone ? "farmer" : who.login}`).toBe(200)
  return ((await r.json()) as { data: { access_token: string } }).data
    .access_token
}

test("a farmer is turned back from staff surfaces and refused by staff routes", async ({
  page,
  request,
}) => {
  const farmer = cred("field (RUAHA)", "farmer")
  await loginField(page, farmer)
  for (const surface of ["/officer", "/ops"]) {
    await page.goto(`${urls.field}${surface}`)
    await expect(page).toHaveURL(
      new RegExp(`${urls.field}/(farm|set-password)$`)
    )
  }
  const token = await tokenOf(request, farmer)
  // A phone-only farmer account is no office user: refused (403) or not even shown it (404).
  for (const path of ["/products", "/app-keys"])
    expect([403, 404], path).toContain(
      await statusAs(request, token, `/workspace/${workspaces.RUAHA}${path}`)
    )
  // Farm records: a farmer reads their own household only (farm_person_persons), never a
  // neighbour's: the neighbour's record by id is refused too.
  const get = async <T>(path: string, as: string) => {
    const r = await request.get(`${urls.api}/v1${path}`, {
      headers: { Authorization: `Bearer ${as}` },
    })
    expect(r.status(), path).toBe(200)
    return (await r.json()) as { data: T }
  }
  const me = await get<{ memberships: { person_id: string | null }[] }>(
    "/farm/me",
    token
  )
  const own = me.data.memberships.find((m) => m.person_id)?.person_id
  const persons = `/workspace/${workspaces.RUAHA}/farm/persons`
  const seen = (
    await get<{ id: string }[]>(`${persons}?page_size=100`, token)
  ).data.map((p) => p.id)
  expect(seen).toContain(own)
  const admin = await apiAs(request, cred("office (RUAHA)", "admin"))
  const all = await admin.get<{ id: string }[]>(`${persons}?page_size=100`)
  expect(seen.length, "a household, not the village").toBeLessThan(all.length)
  const neighbour = all.find((p) => !seen.includes(p.id))!.id
  expect([403, 404]).toContain(
    await statusAs(request, token, `${persons}/${neighbour}`)
  )
})

test("a field officer is turned back from ops and refused ops-only work", async ({
  page,
  request,
}) => {
  const officer = cred("field (RUAHA)", "field_officer")
  await loginField(page, officer)
  await page.goto(`${urls.field}/ops`)
  await expect(page).toHaveURL(`${urls.field}/officer`)
  const token = await tokenOf(request, officer)
  // Admin-only account settings stay shut to a farm officer.
  expect(
    await statusAs(request, token, `/workspace/${workspaces.RUAHA}/app-keys`)
  ).toBe(403)
  expect(
    await statusAs(
      request,
      token,
      `/workspace/${workspaces.RUAHA}/webhook-endpoints`
    )
  ).toBe(403)
})

// The member's overview and settings fire many calls the API refuses (S5); all 403, none other.
refusing.describe("a non-admin member in the office", () => {
  refusing.use({
    refused: {
      prefix: `${urls.office}/v1/workspace/${workspaces.RUAHA}/`,
      statuses: [403],
    },
  })

  refusing(
    "sees app keys refused in words, with no keys and no code",
    async ({ page }) => {
      const officer = cred("field (RUAHA)", "field_officer")
      await loginOffice(page, officer)
      await page.goto(`${urls.office}/settings/app-keys`)
      await expect(
        page
          .getByRole("alert")
          .filter({ hasText: "This action is unauthorized." })
      ).toBeVisible({ timeout: 20_000 })
      await expect(page.getByText(/gsk_/)).toHaveCount(0)
      await expect(page.getByText(/\b403\b/)).toHaveCount(0)
    }
  )
})
