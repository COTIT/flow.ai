import { expect, test } from '@playwright/test'
import { tr } from './support/i18n'
import { chooseSelect } from './support/select'

/**
 * Spec 4.1: "Language switch, persisted to app_user.locale."
 *
 * app_user.locale is the source of truth, not localStorage — the choice has to
 * follow the user to the next device. The seed gives the officers and farmers
 * locale 'sw' and the ops/admin accounts 'en', so the applied locale is
 * observable straight after sign-in.
 *
 * Note these tests assert the SELECTED LOCALE, not translated copy. sw is now
 * a draft Swahili bundle, and the language names below are asserted through
 * tr(lng, key) so they follow it. The switch names each language in the
 * language currently shown: under sw the English option reads
 * tr('sw', 'language.en'), under en the Swahili option reads
 * tr('en', 'language.sw').
 */
const PASSWORD = 'demo1234'

/** The PATCH that stores the choice on app_user.locale. */
function localeSaved(page: import('@playwright/test').Page) {
  return page.waitForResponse(
    (r) => r.url().includes('/rest/v1/app_user') && r.request().method() === 'PATCH',
  )
}

async function signIn(page: import('@playwright/test').Page, email: string) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
}

test('the session applies app_user.locale on sign-in', async ({ page }) => {
  // Salima Officer is seeded with locale 'sw'.
  await signIn(page, 'officer.ilundo@demo.ruaha360.test')
  await expect(page).toHaveURL(/\/officer$/)

  await expect(page.getByTestId('language-switch')).toContainText(tr('sw', 'language.sw'))
})

test('an account seeded as English stays English', async ({ page }) => {
  // Asha Ops is seeded with locale 'en'.
  await signIn(page, 'ops@demo.ruaha360.test')
  await expect(page).toHaveURL(/\/ops$/)

  await expect(page.getByTestId('language-switch')).toContainText(tr('en', 'language.en'))
})

test('a language change survives a reload, because it is stored on app_user', async ({ page }) => {
  await signIn(page, 'officer.ilundo@demo.ruaha360.test')
  await expect(page).toHaveURL(/\/officer$/)
  await expect(page.getByTestId('language-switch')).toContainText(tr('sw', 'language.sw'))

  const select = page.getByTestId('language-switch')

  // Wait for the write itself, not for the switch to look idle. The switch only
  // disables after changeLanguage re-renders, so "enabled" can hold before the
  // PATCH is even sent — and a reload then cuts the write. Registered before
  // the change so the response cannot slip past.
  const toEnglish = localeSaved(page)
  await chooseSelect(page, 'language-switch', tr('sw', 'language.en'))
  await expect(select).toContainText(tr('en', 'language.en'))
  expect((await toEnglish).ok()).toBe(true)

  await page.reload()
  await expect(select).toContainText(tr('en', 'language.en'))

  // Leave the seed as it was found, so the suite is re-runnable.
  const toSwahili = localeSaved(page)
  await chooseSelect(page, 'language-switch', tr('en', 'language.sw'))
  await expect(select).toContainText(tr('sw', 'language.sw'))
  expect((await toSwahili).ok()).toBe(true)
  await page.reload()
  await expect(select).toContainText(tr('sw', 'language.sw'))
})
