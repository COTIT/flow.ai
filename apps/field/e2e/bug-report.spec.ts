import { expect, test } from '@playwright/test'

import { markedName } from './support/marker'

/**
 * The bug button: on every signed-in screen, it opens a sheet already holding
 * a screenshot and the steps taken before the tap; a description is required.
 * The filed report is marked, so global teardown removes it and its images.
 */
test('a farmer files a bug report with a screenshot and their last steps', async ({ page }) => {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
  await page.getByTestId('login-password').fill('demo1234')
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/farm$/)

  await page.getByTestId('bug-fab').click()
  const sheet = page.getByTestId('bug-sheet')
  await expect(sheet).toBeVisible()
  await expect(sheet.getByTestId('bug-screenshot')).toBeVisible()
  // Drawn, not just present: a revoked preview link renders as a broken image.
  await expect
    .poll(() => sheet.getByTestId('bug-screenshot').evaluate((img: HTMLImageElement) => img.naturalWidth))
    .toBeGreaterThan(0)
  // The sign-in itself is on the trail: the click, the token call, the navigation.
  await sheet.getByTestId('bug-actions').locator('summary').click()
  await expect(sheet.getByTestId('bug-actions')).toContainText('[login-submit]')
  await expect(sheet.getByTestId('bug-actions')).toContainText('/auth/v1/token?grant_type')
  await expect(sheet.getByTestId('bug-actions')).not.toContainText('demo1234')

  await sheet.getByTestId('bug-submit').click()
  await expect(sheet.getByTestId('bug-description-error')).toBeVisible()

  await sheet.getByTestId('bug-description').fill(`${markedName()} the survey card did not open`)
  await sheet.getByTestId('bug-submit').click()
  await expect(page.getByTestId('bug-sent')).toBeVisible()
  await expect(sheet).toHaveCount(0)
})
