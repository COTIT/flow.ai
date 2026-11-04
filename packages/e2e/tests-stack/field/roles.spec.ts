import { cred, expect, test, urls } from "../../src/stack"
import { farmApi, signIn, who } from "./field"

const home = { farmer: /\/farm$/, field_officer: /\/officer$/, ops: /\/ops$/ }
const roles = {
  farmer: who("farmer", "0714003001"),
  field_officer: who("field_officer", "juma@ruaha-demo.test"),
  ops: cred("field (RUAHA)", "ops"),
}

// Signing in lands each seeded role on its own home; a screen of another role sends them back there.
const outside: [keyof typeof roles, string[]][] = [
  ["farmer", ["/ops/dispatch", "/officer/verify", "/ops/tower"]],
  ["field_officer", ["/ops/dispatch", "/ops/villages", "/farm/passbook"]],
  ["ops", ["/farm", "/farm/passbook"]],
]

for (const [role, paths] of outside) {
  test(`a ${role} lands on their home and is turned back from other roles' screens`, async ({ page, request }) => {
    await signIn(page, request, roles[role])
    await expect(page).toHaveURL(home[role])
    for (const path of paths) {
      await page.goto(`${urls.field}${path}`)
      await expect(page, path).toHaveURL(home[role])
    }
  })
}

// The guard is UX; the API is the boundary. Each role's token is refused, or scoped, by its role.
test("the API scopes and refuses each role by its role", async ({ request }) => {
  const farmer = await farmApi(request, roles.farmer)
  const officer = await farmApi(request, roles.field_officer)
  type Person = { id: string; village_id: string }

  // A farmer reads only their own person, and no buying or dispatch.
  const mine = await farmer.get<Person[]>("/farm/persons?page_size=100")
  expect(mine.data.map((p) => p.id)).toEqual([farmer.me.memberships[0]!.person_id])
  expect((await farmer.call("GET", "/farm/supply-batches?page_size=1")).status).toBe(403)
  expect((await farmer.call("GET", "/farm/dispatch/options")).status).toBe(403)

  // An officer reads only their own village's people, and does not dispatch.
  const village = officer.me.memberships[0]!.village_id
  const people = await officer.get<Person[]>("/farm/persons?page_size=100")
  expect(people.data.length).toBeGreaterThan(0)
  expect(new Set(people.data.map((p) => p.village_id))).toEqual(new Set([village]))
  expect((await officer.call("GET", "/farm/dispatch/options")).status).toBe(403)
})
