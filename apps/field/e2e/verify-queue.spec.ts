import { expect, test, type Page } from '@playwright/test'

import { markedName } from './support/marker'
import { markedPhone } from './support/phone'
import { CROP } from './support/seed'

const PASSWORD = 'demo1234'
const OFFICER = 'officer.ilundo@demo.ruaha360.test'

async function signInAsOfficer(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(OFFICER)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/officer$/)
}

async function registerMarkedFarmer(page: Page) {
  const family = markedName()
  await page.goto('/officer/register')
  await page.getByTestId('register-given-name').fill('Queue')
  await page.getByTestId('register-family-name').fill(family)
  await page.getByTestId('register-phone').fill(markedPhone())
  await page.getByTestId('register-household-label').fill(`${family} household`)
  await page.getByTestId('register-farm-label').fill(`${family} farm`)
  await page.getByTestId('register-plot-label').fill(`${family} plot`)
  await page.getByTestId('register-plot-area').fill('1.5')
  await page.getByTestId(`register-crop-${CROP.MAIZE.id}`).check()
  await page.getByTestId('register-cycle-area').fill('1.2')
  await page.getByTestId('register-harvest-start').fill('2026-09-01')
  await page.getByTestId('register-harvest-end').fill('2026-09-30')
  await page.getByTestId('register-harvest-kg').fill('3000')
  await page.getByTestId('register-submit').click()
  await expect(page.getByTestId('register-success')).toBeVisible()
  await page.getByTestId('register-view-person').click()
  await expect(page.getByTestId('person-detail')).toBeVisible()

  const idFromTestId = async (prefix: string) => {
    const testId = await page.locator(`[data-testid^="${prefix}-"]`).first().getAttribute('data-testid')
    expect(testId).toBeTruthy()
    return testId!.slice(prefix.length + 1)
  }

  return {
    personId: new URL(page.url()).pathname.split('/').pop()!,
    farmId: await idFromTestId('farm'),
    plotId: await idFromTestId('plot'),
    cycleId: await idFromTestId('cycle'),
    harvestId: await idFromTestId('harvest'),
  }
}

test.describe('/officer/verify', () => {
  test('links every record type to contextual detail and protects verification', async ({ page }) => {
    await signInAsOfficer(page)
    const ids = await registerMarkedFarmer(page)

    await page.goto('/officer/verify')
    await expect(page.getByTestId('verify-queue')).toBeVisible()
    await expect(page.getByTestId('verify-queue-row')).not.toHaveCount(0)
    const rowCount = await page.getByTestId('verify-queue-row').count()

    // Cancel leaves the exact row and action untouched.
    await page.getByTestId(`verify-person-${ids.personId}`).click()
    await expect(page.getByRole('alertdialog')).toBeVisible()
    await page.getByTestId('confirm-dialog-cancel').click()
    await expect(page.getByTestId(`verify-person-${ids.personId}`)).toBeVisible()
    await expect(page.getByTestId('verify-queue-row')).toHaveCount(rowCount)

    const targets = [
      {
        table: 'person',
        href: `/officer/people/${ids.personId}`,
        focused: '[data-testid="person-detail"]',
      },
      {
        table: 'farm',
        href: `/officer/farms/${ids.farmId}`,
        focused: '[data-testid="farm-detail"]',
      },
      {
        table: 'plot',
        href: `/officer/farms/${ids.farmId}?plot=${ids.plotId}`,
        focused: `[data-record-id="${ids.plotId}"][data-focused="true"]`,
      },
      {
        table: 'crop_cycle',
        href: `/officer/cycles/${ids.cycleId}`,
        focused: '[data-testid="cycle-detail"]',
      },
      {
        table: 'harvest_report',
        href: `/officer/cycles/${ids.cycleId}?harvest=${ids.harvestId}`,
        focused: `[data-record-id="${ids.harvestId}"][data-focused="true"]`,
      },
    ] as const

    for (const target of targets) {
      const link = page.locator(`[data-table="${target.table}"] a[href="${target.href}"]`)
      await expect(link).toHaveCount(1)
      await link.click()
      await expect(page).toHaveURL(`http://localhost:5173${target.href}`)
      await expect(page.locator(target.focused)).toBeVisible()
      await page.goto('/officer/verify')
      await expect(page.getByTestId('verify-queue')).toBeVisible()
    }
  })
})
