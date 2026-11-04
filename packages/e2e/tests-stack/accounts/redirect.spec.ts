import { cred, expect, test, urls } from "../../src/stack"

for (const next of [
  "//evil.example.com",
  "https://evil.example.com",
  "/%2F%2Fevil.example.com",
])
  test(`office login with next=${next} lands on the overview`, async ({
    page,
  }) => {
    const owner = cred("office (AFM)", "owner")
    await page.goto(`${urls.office}/login?next=${encodeURIComponent(next)}`)
    await page.getByLabel("Email").fill(owner.login)
    await page.getByLabel("Password").fill(owner.secret)
    await page.getByRole("button", { name: "Log in" }).click()
    await expect(page).toHaveURL(`${urls.office}/`)
    await expect(
      page.getByRole("heading", { level: 1, name: "Overview" })
    ).toBeVisible()
  })

for (const target of ["//evil.example.com", "https://evil.example.com"])
  test(`field sign-in with redirect=${target} stays on Field`, async ({
    page,
  }) => {
    const officer = cred("field (RUAHA)", "field_officer")
    await page.goto(
      `${urls.field}/login?redirect=${encodeURIComponent(target)}`
    )
    await page.getByLabel("Phone number or email").fill(officer.login)
    await page.getByLabel("Password").fill(officer.secret)
    await page.getByRole("button", { name: "Sign in" }).click()
    await expect(page).toHaveURL(`${urls.field}/officer`)
  })
