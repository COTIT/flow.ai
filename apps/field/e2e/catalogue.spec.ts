import { expect, test, type Page } from '@playwright/test'

import { markedName } from './support/marker'
import { chooseSelect } from './support/select'

/**
 * Spec 7.4 — the resource catalogue. Ops adds equipment (rent and/or buy) and
 * loan listings. Rows created here are marked through their code, so
 * cleanup.sql removes them.
 */
const PASSWORD = 'demo1234'

async function signInAsOps(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/ops$/)
}

test.describe('/ops/catalogue', () => {
  test('ops adds a machine offered for rent, and it is listed', async ({ page }) => {
    const code = markedName()
    await signInAsOps(page)
    await page.goto('/ops/catalogue')
    await page.getByTestId('catalogue-create-open').click()

    await page.getByTestId('catalogue-code').fill(code)
    await page.getByTestId('catalogue-name-en').fill(`${code} hammer mill`)
    await page.getByTestId('catalogue-name-sw').fill(`${code} mashine ya nyundo`)
    await chooseSelect(page, 'catalogue-category', 'Milling')
    await page.getByTestId('catalogue-power').fill('5.5')
    await page.getByTestId('catalogue-can-rent').check()
    await page.getByTestId('catalogue-can-buy').uncheck()
    await page.getByTestId('catalogue-rent-price').fill('60000')
    await page.getByTestId('catalogue-create-submit').click()

    const row = page.getByTestId('catalogue-row').filter({ hasText: `${code} hammer mill` })
    await expect(row).toBeVisible()
    await expect(row.getByTestId('catalogue-offered')).toHaveText(/rent/i)
    await expect(row.getByTestId('catalogue-rent')).toContainText('TZS 60,000.00')
  })

  test('a machine offered neither way is refused with a readable message', async ({ page }) => {
    const code = markedName()
    await signInAsOps(page)
    await page.goto('/ops/catalogue')
    await page.getByTestId('catalogue-create-open').click()

    await page.getByTestId('catalogue-code').fill(code)
    await page.getByTestId('catalogue-name-en').fill(code)
    await page.getByTestId('catalogue-name-sw').fill(code)
    await chooseSelect(page, 'catalogue-category', 'Milling')
    await page.getByTestId('catalogue-can-buy').uncheck()
    await page.getByTestId('catalogue-create-submit').click()

    await expect(page.getByTestId('catalogue-create-error')).toContainText(/rent, to buy, or both/i)
  })

  test('ops adds a loan listing, and the farmer sees it', async ({ page }) => {
    const code = markedName()
    await signInAsOps(page)
    await page.goto('/ops/catalogue?kind=loan')
    await page.getByTestId('catalogue-create-open').click()

    await page.getByTestId('catalogue-code').fill(code)
    await page.getByTestId('catalogue-name-en').fill(`${code} women group loan`)
    await page.getByTestId('catalogue-name-sw').fill(`${code} mkopo wa kikundi`)
    await page.getByTestId('catalogue-min').fill('500000')
    await page.getByTestId('catalogue-max').fill('3000000')
    await page.getByTestId('catalogue-create-submit').click()

    const row = page.getByTestId('catalogue-loan-row').filter({ hasText: code })
    await expect(row).toBeVisible()
    await expect(row.getByTestId('catalogue-loan-range')).toContainText('TZS 500,000.00 – TZS 3,000,000.00')

    await page.getByTestId('sign-out').click()
    await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/farm$/)
    await page.goto('/farm/equipment?kind=loan')
    await expect(page.getByTestId('loan-item').filter({ hasText: code })).toBeVisible()
  })
})
