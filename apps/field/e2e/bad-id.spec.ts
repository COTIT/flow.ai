import { expect, test, type Page } from '@playwright/test'

/**
 * QA #3 — a malformed id must not show a Postgres parse failure.
 *
 * `/officer/people/not-a-uuid` rendered
 * `invalid input syntax for type uuid: "not-a-uuid"`, on every `$id` route.
 *
 * The target behaviour already exists for a WELL-FORMED id that matches
 * nothing: RLS returns zero rows and the screen says "not found · It may not
 * exist, or you may not have access to it." A malformed id should reach the
 * same place. The contrast that shows the fix was already understood
 * elsewhere: `/ops/tower?village=nonsense` renders the village prompt, because
 * search params are validated and route params were not.
 *
 * Read-only: visits URLs and asserts what renders. Creates nothing.
 */
const PASSWORD = 'demo1234'

const ACCOUNT = {
  officer: { email: 'officer.ilundo@demo.ruaha360.test', home: /\/officer$/ },
  farmer: { email: 'neema@demo.ruaha360.test', home: /\/farm$/ },
  ops: { email: 'ops@demo.ruaha360.test', home: /\/ops$/ },
} as const

/** Every `$id` route in the app, with the surface that can reach it. */
const ROUTES: Array<{ surface: keyof typeof ACCOUNT; path: (id: string) => string }> = [
  { surface: 'officer', path: (id) => `/officer/people/${id}` },
  { surface: 'officer', path: (id) => `/officer/farms/${id}` },
  { surface: 'officer', path: (id) => `/officer/cycles/${id}` },
  { surface: 'farmer', path: (id) => `/farm/requests/${id}` },
  { surface: 'farmer', path: (id) => `/farm/equipment/${id}` },
  { surface: 'ops', path: (id) => `/ops/requests/${id}` },
  { surface: 'ops', path: (id) => `/ops/demand/${id}` },
  { surface: 'ops', path: (id) => `/ops/opportunities/${id}` },
]

/** Well formed, and matching nothing. The behaviour a bad id should reach. */
const ABSENT = '00000000-0000-4000-8000-0000000000ff'
const MALFORMED = 'not-a-uuid'

async function signIn(page: Page, surface: keyof typeof ACCOUNT) {
  const account = ACCOUNT[surface]
  await page.goto('/login')
  await page.getByTestId('login-email').fill(account.email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(account.home)
}

for (const route of ROUTES) {
  const sample = route.path(':id')

  test(`${sample} renders not-found for a malformed id`, async ({ page }) => {
    await signIn(page, route.surface)
    await page.goto(route.path(MALFORMED))

    await expect(page.getByTestId('empty-state')).toBeVisible()
    await expect(page.getByTestId('error-state')).toHaveCount(0)
    // Nothing of the parse failure, and no database internals.
    await expect(page.locator('main')).not.toContainText(/uuid|invalid input syntax/i)
  })

  // The same screen, the same state: a bad id is not a different kind of
  // absence from an id that simply is not there.
  test(`${sample} treats a malformed id like an absent one`, async ({ page }) => {
    await signIn(page, route.surface)

    await page.goto(route.path(ABSENT))
    const absent = await page.getByTestId('empty-state').textContent()

    await page.goto(route.path(MALFORMED))
    await expect(page.getByTestId('empty-state')).toHaveText(absent!)
  })
}
