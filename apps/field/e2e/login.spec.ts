import { expect, test } from '@playwright/test'

import { tr } from './support/i18n'

/**
 * Spec 4.2 acceptance criterion, verbatim:
 *   "each of the six seeded accounts lands on the correct home"
 *
 * Runs against the seeded demo database. The password is the same for every
 * demo account and is published in supabase/seed.sql — it guards nothing.
 */
const PASSWORD = 'demo1234'

const ACCOUNTS = [
  { email: 'admin@demo.ruaha360.test', home: '/ops', role: 'admin' },
  { email: 'ops@demo.ruaha360.test', home: '/ops', role: 'ops' },
  { email: 'officer.ilundo@demo.ruaha360.test', home: '/officer', role: 'field_officer' },
  { email: 'officer.mgama@demo.ruaha360.test', home: '/officer', role: 'field_officer' },
  { email: 'neema@demo.ruaha360.test', home: '/farm', role: 'farmer' },
  { email: 'joseph@demo.ruaha360.test', home: '/farm', role: 'farmer' },
] as const

test.describe('login and membership routing', () => {
  for (const account of ACCOUNTS) {
    test(`${account.role} ${account.email} lands on ${account.home}`, async ({ page }) => {
      await page.goto('/login')

      await page.getByTestId('login-email').fill(account.email)
      await page.getByTestId('login-password').fill(PASSWORD)
      await page.getByTestId('login-submit').click()

      await expect(page).toHaveURL(new RegExp(`${account.home}$`))
    })
  }

  test('wrong password shows an inline error and stays on /login', async ({ page }) => {
    await page.goto('/login')

    await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
    await page.getByTestId('login-password').fill('definitely-not-the-password')
    await page.getByTestId('login-submit').click()

    await expect(page.getByTestId('login-error')).toBeVisible()
    await expect(page).toHaveURL(/\/login$/)
  })

  test('the demo banner is visible once signed in', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()

    await expect(page).toHaveURL(/\/ops$/)
    // Driven by VITE_DATA_MODE, never by a database column.
    await expect(page.getByTestId('demo-banner')).toBeVisible()
  })

  test('signing out returns to /login and the session does not survive', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/ops$/)

    await page.getByTestId('sign-out').click()
    await expect(page).toHaveURL(/\/login$/)

    // Route guards are UX, so the guard should bounce an unauthenticated
    // visit. It also records where the visitor was heading.
    await page.goto('/ops')
    await expect(page).toHaveURL(/\/login(\?|$)/)
    await expect(page.getByTestId('login-submit')).toBeVisible()
  })

  test('a guarded deep link is honoured after signing in', async ({ page }) => {
    await page.goto('/ops/tower')
    await expect(page).toHaveURL(/\/login\?redirect=/)

    await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()

    await expect(page).toHaveURL(/\/ops\/tower$/)
  })

  test('a farmer deep-linking into ops is sent to their own surface', async ({ page }) => {
    await page.goto('/ops/tower')
    await expect(page).toHaveURL(/\/login\?redirect=/)

    await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()

    // The redirect is attempted, then the ops guard corrects it. RLS would
    // have returned zero rows anyway; this is the UX half.
    await expect(page).toHaveURL(/\/farm$/)
  })
})

/**
 * QA #26 and #10. `/login` rendered the sign-in form AND the header's "Sign
 * out" button for someone already signed in — one route showing two mutually
 * exclusive states. `/select-role` told a single-role officer "You hold more
 * than one role. Pick the one you want to work in", above a single option.
 *
 * Neither is reachable by navigation; both are reachable by typing a URL, and
 * `resolveLanding` already knows where each user belongs.
 */
test.describe('routes a signed-in user should not be shown', () => {
  test('/login sends a signed-in user to their own surface', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
    await page.getByTestId('login-password').fill('demo1234')
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/ops$/)

    await page.goto('/login')

    await expect(page).toHaveURL(/\/ops$/)
    await expect(page.getByTestId('login-email')).toHaveCount(0)
  })

  test('/select-role does not claim a choice a single-role user does not have', async ({
    page,
  }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('officer.ilundo@demo.ruaha360.test')
    await page.getByTestId('login-password').fill('demo1234')
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/officer$/)

    await page.goto('/select-role')

    await expect(page).toHaveURL(/\/officer$/)
    await expect(page.locator('main')).not.toContainText(/more than one role/i)
    // Salima is a 'sw' user: the same sentence, from the bundle.
    await expect(page.locator('main')).not.toContainText(tr('sw', 'selectRole.detail'))
  })

  // Signing out still has to reach the form, or there is no way back in.
  test('signing out still lands on a usable sign-in form', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
    await page.getByTestId('login-password').fill('demo1234')
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/ops$/)

    await page.getByTestId('sign-out').click()

    await expect(page).toHaveURL(/\/login$/)
    await expect(page.getByTestId('login-email')).toBeVisible()
  })
})
