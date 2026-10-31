import { expect, test, type Page } from '@playwright/test'

import { tr } from './support/i18n'
import { markedName } from './support/marker'
import { markedPhone } from './support/phone'
import { CROP } from './support/seed'

/**
 * Spec 5.4 — person detail with provenance on every record, plus verify.
 *
 * Verification is exercised against records this suite CREATES, never against
 * seeded ones. The seed's mix of verified, pending and unverified records is
 * deliberate demo content, and app_verify is one-way — there is no unverify —
 * so flipping a seeded record would permanently degrade the demo.
 */
const PASSWORD = 'demo1234'

async function signInAsOfficer(page: Page) {
  await signIn(page, 'officer.ilundo@demo.ruaha360.test', /\/officer$/)
}

async function signIn(page: Page, email: string, home: RegExp) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(home)
}

async function confirmVerification(page: Page) {
  await page.getByTestId('confirm-dialog-confirm').click()
}

/** Registers a marked farmer and lands on their detail screen. */
async function registerAndOpen(page: Page): Promise<string> {
  const family = markedName()
  await page.goto('/officer/register')
  await page.getByTestId('register-given-name').fill('Test')
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
  return family
}

test.describe('/officer/people/$personId', () => {
  test('shows the whole record graph the registration created', async ({ page }) => {
    await signInAsOfficer(page)
    const family = await registerAndOpen(page)

    await expect(page.getByTestId('person-detail')).toContainText(family)
    await expect(page.getByTestId('person-detail')).toContainText(`${family} household`)
    await expect(page.getByTestId('person-detail')).toContainText(`${family} farm`)
    await expect(page.getByTestId('person-detail')).toContainText(`${family} plot`)
    // Crop name comes from the database in the officer's own language.
    await expect(page.getByTestId('person-detail')).toContainText(CROP.MAIZE.sw)
    await expect(page.getByTestId('person-detail')).toContainText('3,000.00 kg')
  })

  // Every record shows where its data came from.
  test('every record carries a provenance badge', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    const badges = page.getByTestId('provenance-badge')
    // person + household + farm + plot + cycle + harvest
    await expect(badges).toHaveCount(6)

    // The RPC stamps the officer's writes as field_verified and the farmer's
    // reported figure as farmer_reported.
    await expect(page.locator('[data-source="field_verified"]').first()).toBeVisible()
    await expect(page.locator('[data-source="farmer_reported"]').first()).toBeVisible()
  })

  test('nothing starts verified, and the outstanding count says so', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    await expect(page.getByTestId('person-outstanding')).toContainText(tr('sw', 'person.unverifiedCount', { count: 6 }))
    await expect(page.locator('[data-verification="unverified"]')).toHaveCount(6)
  })

  // The acceptance criterion for this screen: verifying flips the badge with
  // no manual refresh, because the mutation invalidates the record key.
  test('verifying flips the badge in place, without a reload', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    const personBadge = page.getByTestId('person-detail').getByTestId('provenance-badge').first()
    await expect(personBadge).toHaveAttribute('data-verification', 'unverified')

    await page.locator('[data-verify-table="person"]').click()
    await confirmVerification(page)

    await expect(personBadge).toHaveAttribute('data-verification', 'verified')
    await expect(page.getByTestId('person-outstanding')).toContainText(tr('sw', 'person.unverifiedCount', { count: 5 }))
  })

  test('a verified record no longer offers a verify control', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    await page.locator('[data-verify-table="person"]').click()
    await confirmVerification(page)
    await expect(page.locator('[data-verify-table="person"]')).toHaveCount(0)
  })

  test('edits every current section, resets verification, and persists after reload', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    const idFromTestId = async (prefix: string) => {
      const testId = await page.locator(`[data-testid^="${prefix}-"]`).first().getAttribute('data-testid')
      expect(testId).toBeTruthy()
      return testId!.slice(prefix.length + 1)
    }
    const householdId = await idFromTestId('household')
    const farmId = await idFromTestId('farm')
    const plotId = await idFromTestId('plot')
    const cycleId = await idFromTestId('cycle')
    const harvestId = await idFromTestId('harvest')

    // A correction to an already verified record must require fresh review.
    await page.locator('[data-verify-table="person"]').click()
    await confirmVerification(page)
    await expect(page.locator('[data-verify-table="person"]')).toHaveCount(0)
    await page.getByTestId('edit-person').click()
    await page.getByTestId('edit-given_name').fill('Corrected')
    await page.getByTestId('officer-edit-save').click()
    await expect(page.getByTestId('officer-edit-dialog')).toHaveCount(0)
    await expect(page.getByTestId('person-detail')).toContainText('Corrected')
    await expect(page.locator('[data-verify-table="person"]')).toHaveCount(1)

    await page.getByTestId(`edit-household-${householdId}`).click()
    await page.getByTestId('edit-label').fill('Corrected household')
    await page.getByTestId('officer-edit-save').click()
    await expect(page.getByTestId('officer-edit-dialog')).toHaveCount(0)

    await page.getByTestId(`edit-farm-${farmId}`).click()
    await page.getByTestId('edit-label').fill('Corrected farm')
    await page.getByTestId('officer-edit-save').click()
    await expect(page.getByTestId('officer-edit-dialog')).toHaveCount(0)

    await page.getByTestId(`edit-plot-${plotId}`).click()
    await page.getByTestId('edit-label').fill('Corrected plot')
    await page.getByTestId('edit-area_ha').fill('2.5')
    await page.getByTestId('officer-edit-save').click()
    await expect(page.getByTestId('officer-edit-dialog')).toHaveCount(0)

    await page.getByTestId(`edit-cycle-${cycleId}`).click()
    await page.getByTestId('edit-season_label').fill('Corrected season')
    await page.getByTestId('officer-edit-save').click()
    await expect(page.getByTestId('officer-edit-dialog')).toHaveCount(0)

    await page.getByTestId(`edit-harvest-${harvestId}`).click()
    await page.getByTestId('edit-quantity_kg').fill('3333')
    await page.getByTestId('officer-edit-save').click()
    await expect(page.getByTestId('officer-edit-dialog')).toHaveCount(0)

    await page.reload()
    await expect(page.getByTestId('person-detail')).toContainText('Corrected household')
    await expect(page.getByTestId('person-detail')).toContainText('Corrected farm')
    await expect(page.getByTestId('person-detail')).toContainText('Corrected plot')
    await expect(page.getByTestId('person-detail')).toContainText('Corrected season')
    await expect(page.getByTestId('person-detail')).toContainText('3,333.00 kg')
    await expect(page.getByTestId('person-detail')).toContainText('3,000.00 kg')
  })

  test('a failed correction keeps entered values and the form actionable', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    await page.route('**/rest/v1/rpc/app_update_observed_record', (route) => route.abort('failed'))
    await page.getByTestId('edit-person').click()
    await page.getByTestId('edit-given_name').fill('Not saved')
    await page.getByTestId('officer-edit-save').click()

    await expect(page.getByTestId('officer-edit-error')).toBeVisible()
    await expect(page.getByTestId('edit-given_name')).toHaveValue('Not saved')
    await expect(page.getByTestId('officer-edit-save')).toBeEnabled()
  })

  // Four eyes on households (business-rules §5): the registering officer
  // verifies five records; the household needs a second staff member.
  test('verifying every record clears the outstanding count', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    await expect(page.locator('[data-verify-table="household"]')).toHaveCount(0)
    for (let i = 0; i < 5; i += 1) {
      const next = page.locator('[data-verify-table]').first()
      if ((await next.count()) === 0) break
      await next.click()
      await confirmVerification(page)
      await expect(page.getByTestId('person-outstanding')).not.toContainText(
        tr('sw', 'person.unverifiedCount', { count: 6 - i }),
      )
    }
    await expect(page.getByTestId('person-outstanding')).toContainText(tr('sw', 'person.unverifiedCount', { count: 1 }))
    await expect(page.locator('[data-verify-table]')).toHaveCount(0)

    const personUrl = page.url()
    await page.getByTestId('sign-out').click()
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto(personUrl)
    await page.locator('[data-verify-table="household"]').click()
    await confirmVerification(page)

    await expect(page.getByTestId('person-outstanding')).toContainText(tr('en', 'person.allVerified'))
    await expect(page.locator('[data-verify-table]')).toHaveCount(0)
  })

  test('a failed verify is reported and the badge does not lie', async ({ page }) => {
    await signInAsOfficer(page)
    await registerAndOpen(page)

    await page.route('**/rest/v1/rpc/app_verify', (route) => route.abort('failed'))
    await page.locator('[data-verify-table="person"]').click()
    await confirmVerification(page)

    await expect(page.getByTestId('verify-error')).toBeVisible()
    const personBadge = page.getByTestId('person-detail').getByTestId('provenance-badge').first()
    await expect(personBadge).toHaveAttribute('data-verification', 'unverified')
  })

  test('a person the officer cannot see is an empty state, not an error', async ({ page }) => {
    await signInAsOfficer(page)
    // A well-formed id that matches nothing visible: RLS returns zero rows.
    await page.goto('/officer/people/60000000-0000-4000-8000-0000000000ff')

    await expect(page.getByTestId('empty-state')).toBeVisible()
    await expect(page.getByTestId('error-state')).toHaveCount(0)
  })

  test('a Mgama person is invisible to the Ilundo officer', async ({ page }) => {
    await signInAsOfficer(page)
    // Zawadi Ngowi, seeded in Mgama.
    await page.goto('/officer/people/60000000-0000-4000-8000-000000000007')

    await expect(page.getByTestId('empty-state')).toBeVisible()
  })
})
