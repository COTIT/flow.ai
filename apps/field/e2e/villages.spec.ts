import { expect, test, type Page } from '@playwright/test'

import { markedName } from './support/marker'

/**
 * Spec 7.9 — ops adds a village with its first planned capacity row.
 *
 * The village is marked through its name, so cleanup.sql removes it and its
 * capacity row. A created village is visible to project-level ops at once.
 */
const PASSWORD = 'demo1234'

async function signInAsOps(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/ops$/)
}

test.describe('/ops/villages create', () => {
  test('an empty form says what is missing', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/villages')
    await page.getByTestId('village-create-open').click()

    await page.getByTestId('village-create-submit').click()
    await expect(page.getByTestId('village-name-error')).toBeVisible()
    await expect(page.getByTestId('village-capacity-error')).toBeVisible()
  })

  test('a created village appears with its planned capacity and basis', async ({ page }) => {
    const name = markedName()
    await signInAsOps(page)
    await page.goto('/ops/villages')
    await page.getByTestId('village-create-open').click()

    await page.getByTestId('village-name').fill(name)
    await page.getByTestId('village-code').fill(name.slice(4, 12).toUpperCase())
    await page.getByTestId('village-latitude').fill('-7.91')
    await page.getByTestId('village-longitude').fill('35.62')
    await page.getByTestId('village-capacity').fill('120')
    await page.getByTestId('village-simultaneity').fill('0.7')
    await page.getByTestId('village-create-submit').click()

    const row = page.getByTestId('village-row').filter({ hasText: name })
    await expect(row).toBeVisible()
    await expect(row).toContainText('120.000 kW')
    await expect(row.getByTestId('village-basis')).toHaveText('Planned')
  })

  test('a duplicate code is refused with a readable message', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/villages')
    await page.getByTestId('village-create-open').click()

    await page.getByTestId('village-name').fill(markedName())
    await page.getByTestId('village-code').fill('ILUNDO')
    await page.getByTestId('village-capacity').fill('10')
    await page.getByTestId('village-create-submit').click()

    await expect(page.getByTestId('village-create-error')).toContainText(/village with that code/i)
  })
})
