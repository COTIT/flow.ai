import { fileURLToPath } from 'node:url'

import { expect, test, type Page } from '@playwright/test'

import { tr } from './support/i18n'
import { markedName } from './support/marker'
import { markedPhone } from './support/phone'
import { CROP } from './support/seed'
import { chooseSelect } from './support/select'

/**
 * Spec 5.2 — "the most important screen in the build". One page, one submit,
 * one RPC. The six states it names are each covered here:
 *   empty · restoring draft · saving · saved-locally-not-submitted ·
 *   submit failed with retry · success
 *
 * Its acceptance criterion, verbatim: "kill the network mid-form, reload, the
 * form comes back with every field intact and a visible 'not yet submitted'
 * badge."
 */
const PASSWORD = 'demo1234'
const OFFICER = 'officer.ilundo@demo.ruaha360.test'

async function signInAsOfficer(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(OFFICER)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/officer$/)
}

/** The PATCH that stores a language choice on app_user.locale. */
function localeSaved(page: Page) {
  return page.waitForResponse(
    (r) => r.url().includes('/rest/v1/app_user') && r.request().method() === 'PATCH',
  )
}

async function fillRegistration(page: Page, family: string, phone = markedPhone()) {
  await page.getByTestId('register-given-name').fill('Test')
  await page.getByTestId('register-family-name').fill(family)
  await page.getByTestId('register-phone').fill(phone)
  await page.getByTestId('register-household-label').fill(`${family} household`)
  await page.getByTestId('register-farm-label').fill(`${family} farm`)
  await page.getByTestId('register-plot-label').fill(`${family} plot`)
  await page.getByTestId('register-plot-area').fill('1.5')
  // Ticked by id, not by label: crop names come from the database per the
  // user's locale, and the seeded Ilundo officer reads Swahili.
  await page.getByTestId(`register-crop-${CROP.MAIZE.id}`).check()
  await page.getByTestId('register-cycle-area').fill('1.2')
  await page.getByTestId('register-harvest-start').fill('2026-09-01')
  await page.getByTestId('register-harvest-end').fill('2026-09-30')
  await page.getByTestId('register-harvest-kg').fill('3000')
  return phone
}

test.describe('/officer/register', () => {
  test('starts empty, with nothing marked as an unsaved draft', async ({ page }) => {
    // Without a GPS position: a granted read fills the farm location on
    // arrival, which is real input and rightly marks the draft.
    await page.context().clearPermissions()
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await expect(page.getByTestId('register-given-name')).toHaveValue('')
    await expect(page.getByTestId('unsaved-draft-badge')).toHaveCount(0)
    await expect(page.getByTestId('register-submit')).toBeEnabled()
  })

  test('reports every required field rather than failing silently', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-given-name-error')).toBeVisible()
    await expect(page.getByTestId('register-family-name-error')).toBeVisible()
    await expect(page.getByTestId('register-farm-label-error')).toBeVisible()
    await expect(page.getByTestId('register-plot-label-error')).toBeVisible()
  })

  // crop.measured_by drives which measure field is asked for. The RPC rejects
  // the wrong one, so the form must not offer it.
  test('the measure field follows how the crop is measured', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const crop = (id: string) => page.getByTestId(`register-crop-${id}`)
    await crop(CROP.MAIZE.id).check()
    await expect(page.getByTestId('register-cycle-area')).toBeVisible()
    await expect(page.getByTestId('register-cycle-tree-count')).toHaveCount(0)

    await crop(CROP.MAIZE.id).uncheck()
    await crop(CROP.AVOCADO.id).check()
    await expect(page.getByTestId('register-cycle-tree-count')).toBeVisible()
    await expect(page.getByTestId('register-cycle-area')).toHaveCount(0)

    await crop(CROP.AVOCADO.id).uncheck()
    await crop(CROP.HONEY.id).check()
    await expect(page.getByTestId('register-cycle-unit-count')).toBeVisible()
  })

  // A farmer grows more than one crop on a plot: each is its own cycle, with
  // its own measure and its own expected harvest.
  test('a farmer with two crops is registered with both', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const family = markedName()
    await fillRegistration(page, family)
    await page.getByTestId(`register-crop-${CROP.AVOCADO.id}`).check()
    const avocado = page.getByTestId('register-cycle-card').nth(1)
    await avocado.getByTestId('register-cycle-tree-count').fill('40')
    await page.getByTestId('register-harvest-row').nth(1).getByTestId('register-harvest-kg').fill('600')
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-success')).toBeVisible()
    await page.getByTestId('register-view-person').click()
    // Either language: the crop names are the database's, in the officer's locale.
    const detail = page.getByTestId('person-detail')
    await expect(detail).toContainText(new RegExp(`${CROP.MAIZE.sw}|${CROP.MAIZE.en}`))
    await expect(detail).toContainText(new RegExp(`${CROP.AVOCADO.sw}|${CROP.AVOCADO.en}`))
    await expect(detail).toContainText('600.00 kg')
  })

  // Title documents: picked at registration, kept through a reload, filed
  // against the new plot, and there on the person's record for the verifier.
  test('a title photo is filed with the plot and shown to staff', async ({ page }) => {
    const title = fileURLToPath(new URL('./fixtures/title-demo.png', import.meta.url))
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await fillRegistration(page, markedName())
    await page.getByTestId('register-plot-documents').setInputFiles(title)
    await expect(page.getByTestId('register-document-item')).toContainText('title-demo.png')

    await page.reload()
    await expect(page.getByTestId('register-document-item')).toContainText('title-demo.png')

    await page.getByTestId('register-submit').click()
    await expect(page.getByTestId('register-documents-filed')).toBeVisible()

    await page.getByTestId('register-view-person').click()
    const doc = page.getByTestId('plot-document')
    await expect(doc).toContainText('title-demo.png')
    await expect(doc).toHaveAttribute('href', /plot-documents/)
  })

  // THE acceptance criterion for this screen.
  test('an interrupted form survives a reload with every field intact', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const family = markedName()
    const phone = await fillRegistration(page, family)

    // An unsubmitted form must LOOK unsubmitted.
    await expect(page.getByTestId('unsaved-draft-badge')).toBeVisible()

    await page.reload()

    await expect(page.getByTestId('register-given-name')).toHaveValue('Test')
    await expect(page.getByTestId('register-family-name')).toHaveValue(family)
    await expect(page.getByTestId('register-phone')).toHaveValue(phone)
    await expect(page.getByTestId('register-farm-label')).toHaveValue(`${family} farm`)
    await expect(page.getByTestId('register-plot-area')).toHaveValue('1.5')
    await expect(page.getByTestId('register-harvest-kg')).toHaveValue('3000')
    await expect(page.getByTestId('unsaved-draft-badge')).toBeVisible()
  })

  test('a failed submit stays on the form, keeps the draft and offers retry', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')
    await fillRegistration(page, markedName())

    // Fail only the RPC, so the failure is the write and nothing else.
    await page.route('**/rest/v1/rpc/app_register_farmer', (route) => route.abort('failed'))
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-error')).toBeVisible()
    await expect(page.getByTestId('unsaved-draft-badge')).toBeVisible()
    await expect(page.getByTestId('register-submit')).toBeEnabled()
    await expect(page.getByTestId('register-given-name')).toHaveValue('Test')
  })

  test('a successful submit confirms, clears the draft, and the person is findable', async ({
    page,
  }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const family = markedName()
    await fillRegistration(page, family)
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-success')).toBeVisible()
    // Cleared only after the RPC returned success.
    await expect(page.getByTestId('unsaved-draft-badge')).toHaveCount(0)

    // The draft must not come back on a fresh visit.
    await page.goto('/officer/register')
    await expect(page.getByTestId('register-given-name')).toHaveValue('')
  })

  // Reference data is translated in the database, not in the i18n JSON —
  // these rows are created at runtime and a repo file cannot translate them.
  test('crop names come from the database in the user own language', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    // The seeded Ilundo officer has app_user.locale = 'sw'. The language names
    // are read from the bundle in the language showing when they are chosen:
    // the English option under sw, the Swahili option under en.
    await expect(page.getByTestId('register-crops')).toContainText(CROP.MAIZE.sw)

    // Each switch is awaited on its PATCH to app_user, not on the control
    // looking idle: the officer's seeded locale must be restored before the
    // context closes, or every later test signs in as the wrong language.
    const toEnglish = localeSaved(page)
    await chooseSelect(page, 'language-switch', tr('sw', 'language.en'))
    expect((await toEnglish).ok()).toBe(true)
    await expect(page.getByTestId('register-crops')).toContainText(CROP.MAIZE.en)

    // Leave the seeded locale as it was found.
    const toSwahili = localeSaved(page)
    await chooseSelect(page, 'language-switch', tr('en', 'language.sw'))
    expect((await toSwahili).ok()).toBe(true)
  })

  /**
   * QA #16, #19, #21, #27, #28 — the batch M6 closed.
   *
   * Every check below is a property of the form's own inputs: a name that is
   * not three spaces, an end date that follows a start date, a number that
   * fits its column. None of them is a copy of a business rule — the RPC still
   * owns the measure match, the provenance stamp and idempotency, and its
   * messages still come back verbatim.
   */
  test('whitespace-only names are refused, and never reach the RPC', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await fillRegistration(page, markedName())
    await page.getByTestId('register-given-name').fill('   ')
    await page.getByTestId('register-family-name').fill('   ')
    await page.getByTestId('register-farm-label').fill('   ')
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-given-name-error')).toBeVisible()
    await expect(page.getByTestId('register-family-name-error')).toBeVisible()
    await expect(page.getByTestId('register-farm-label-error')).toBeVisible()
    // Still on the form, and nothing was created.
    await expect(page.getByTestId('register-success')).toHaveCount(0)
  })

  test('a backwards harvest window is caught inline, not as a constraint name', async ({
    page,
  }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await fillRegistration(page, markedName())
    await page.getByTestId('register-harvest-start').fill('2026-09-30')
    await page.getByTestId('register-harvest-end').fill('2026-09-01')
    await page.getByTestId('register-submit').click()

    const error = page.getByTestId('register-harvest-end-error')
    await expect(error).toBeVisible()
    await expect(error).not.toContainText('cycle_window_sane')
    await expect(page.getByTestId('register-error')).toHaveCount(0)
  })

  test('a number too large for its column is refused before the overflow', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await fillRegistration(page, markedName())
    await page.getByTestId('register-harvest-kg').fill('999999999999')
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-harvest-quantity-kg-error')).toBeVisible()
    // The old path returned "numeric field overflow", naming no field at all.
    await expect(page.getByTestId('register-error')).toHaveCount(0)
  })

  test('the measure field is required by the crop that uses it', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await fillRegistration(page, markedName())
    await page.getByTestId('register-cycle-area').fill('')
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-cycle-area-ha-error')).toBeVisible()
    // The RPC's own "this crop is measured by area" prose is no longer how a
    // farmer's officer learns this.
    await expect(page.getByTestId('register-error')).toHaveCount(0)
  })

  // QA #27: `hectares` is numeric(10,4), so this is stored as 1.2346.
  test('says what a column scale will do to what was typed', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await page.getByTestId('register-plot-area').fill('1.23456789')
    await expect(page.getByTestId('register-plot-area-rounded')).toContainText('1.2346')

    await page.getByTestId('register-plot-area').fill('1.5')
    await expect(page.getByTestId('register-plot-area-rounded')).toHaveCount(0)
  })

  // QA #28: person.phone is free text by design, so a hint is the whole fix.
  test('the phone field hints at the format without enforcing one', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await expect(page.getByTestId('register-phone-hint')).toContainText('+255')
  })

  // The trim has to reach the database, not just the validator: " Test "
  // stored as-is is a name nobody can search for.
  test('a name typed with surrounding spaces is stored trimmed', async ({ page }) => {
    const family = markedName()
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    await fillRegistration(page, family)
    await page.getByTestId('register-given-name').fill('  Padded  ')
    await page.getByTestId('register-submit').click()

    await expect(page.getByTestId('register-success')).toBeVisible()
    await page.goto('/officer/people')
    await page.getByTestId('people-search').fill(family)
    await expect(page.getByTestId('people-table')).toContainText(`Padded ${family}`)
  })

  test('a farmer cannot reach the register screen', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/farm$/)

    await page.goto('/officer/register')
    // The officer guard sends them back to their own surface.
    await expect(page).toHaveURL(/\/farm$/)
  })
})
