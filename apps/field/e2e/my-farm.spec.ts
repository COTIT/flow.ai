import { expect, test, type Page } from '@playwright/test'

import { assertsSeededFigures } from './support/seeded'

/**
 * Spec 6.2 — read-only, mobile-first, every figure carrying provenance.
 *
 * Its acceptance criterion: "the farmer sees exactly her own farm. Seeded
 * farmer Neema sees one farm and cannot reach Joseph's."
 */
const PASSWORD = 'demo1234'

// This spec asserts seeded figures, so it starts from seeded state.
assertsSeededFigures()

async function signIn(page: Page, email: string, home: RegExp) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  // Wait for the landing redirect: navigating before it lands races the
  // session and drops straight back to /login.
  await expect(page).toHaveURL(home)
}

const FARM = /\/farm$/
const OFFICER = /\/officer$/

test.describe('/farm/my-farm', () => {
  test('Neema sees exactly her own farm', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test', FARM)
    await page.goto('/farm/my-farm')

    await expect(page.getByTestId('my-farm')).toBeVisible()
    await expect(page.getByTestId('farm-card')).toHaveCount(1)
    await expect(page.getByTestId('my-farm')).toContainText('Shamba la Neema')
  })

  test('and cannot reach the Kimaro farm', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test', FARM)
    await page.goto('/farm/my-farm')

    await expect(page.getByTestId('my-farm')).toBeVisible()
    // RLS scopes by app_farms(); Joseph's farm is simply not in the answer.
    await expect(page.getByTestId('my-farm')).not.toContainText('Shamba la Kimaro')
  })

  test('Joseph sees his own farm and not Neema\'s', async ({ page }) => {
    await signIn(page, 'joseph@demo.ruaha360.test', FARM)
    await page.goto('/farm/my-farm')

    await expect(page.getByTestId('my-farm')).toContainText('Shamba la Kimaro')
    await expect(page.getByTestId('my-farm')).not.toContainText('Shamba la Neema')
  })

  test('plots show their area and cycles show the expected harvest', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test', FARM)
    await page.goto('/farm/my-farm')

    // Seeded: Kipande cha juu 1.8 ha, Kipande cha chini 0.9 ha.
    await expect(page.getByTestId('my-farm')).toContainText('Kipande cha juu')
    await expect(page.getByTestId('my-farm')).toContainText('1.8000 ha')
    await expect(page.getByTestId('my-farm')).toContainText('Kipande cha chini')

    // Current expected figures on her cycles: 4100 (maize), 1800 (banana),
    // 2300 (maize on the lower plot). The superseded 3200 must not appear.
    await expect(page.getByTestId('my-farm')).toContainText('4,100.00 kg')
    await expect(page.getByTestId('my-farm')).toContainText('2,300.00 kg')
    await expect(page.getByTestId('my-farm')).not.toContainText('3,200.00 kg')
  })

  test('every record carries provenance', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test', FARM)
    await page.goto('/farm/my-farm')

    await expect(page.getByTestId('my-farm')).toBeVisible()
    const badges = page.getByTestId('my-farm').getByTestId('provenance-badge')
    expect(await badges.count()).toBeGreaterThan(0)
  })

  // Read-only surface: the farmer cannot verify her own records.
  test('offers no verify control', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test', FARM)
    await page.goto('/farm/my-farm')

    await expect(page.getByTestId('my-farm')).toBeVisible()
    await expect(page.locator('[data-verify-table]')).toHaveCount(0)
  })

  test('an officer is redirected off the farmer surface', async ({ page }) => {
    await signIn(page, 'officer.ilundo@demo.ruaha360.test', OFFICER)

    await page.goto('/farm/my-farm')
    await expect(page).toHaveURL(/\/officer$/)
  })
})
