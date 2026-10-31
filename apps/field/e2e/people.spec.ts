import { expect, test, type Page } from '@playwright/test'

import { tr } from './support/i18n'

const PASSWORD = 'demo1234'

async function signInAsOfficer(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('officer.ilundo@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/officer$/)
}

for (const width of [320, 375, 768, 1024, 1440] as const) {
  test.describe(`/officer/people at ${width}px`, () => {
    test.use({ viewport: { width, height: 900 } })

    test('keeps the white table and all navigation inside the viewport', async ({ page }) => {
      await signInAsOfficer(page)
      await page.goto('/officer/people')

      await expect(page.getByRole('heading', { name: tr('sw', 'people.title') })).toBeVisible()
      await expect(page.getByTestId('people-table')).toBeVisible()
      await expect(page.getByTestId('people-row').first()).toContainText('Joseph Kimaro')
      await expect(page.getByTestId('people-search')).toBeVisible()
      await expect(page.getByTestId('people-filter-verification')).toBeVisible()

      const viewportWidth = page.viewportSize()!.width
      const surface = page.getByTestId('people-table').locator('xpath=../..')
      const tableWrapper = page.getByTestId('people-table').locator('xpath=..')
      const surfaceBox = await surface.boundingBox()
      const wrapperBox = await tableWrapper.boundingBox()
      const navBox = await page.getByTestId('nav-tabs').boundingBox()

      expect(surfaceBox).not.toBeNull()
      expect(wrapperBox).not.toBeNull()
      expect(navBox).not.toBeNull()
      expect(surfaceBox!.x + surfaceBox!.width).toBeLessThanOrEqual(viewportWidth + 1)
      expect(wrapperBox!.x + wrapperBox!.width).toBeLessThanOrEqual(viewportWidth + 1)
      expect(navBox!.x + navBox!.width).toBeLessThanOrEqual(viewportWidth + 1)

      const tableBackground = await surface.evaluate((element) => getComputedStyle(element).backgroundColor)
      expect(tableBackground).toBe('rgb(255, 255, 255)')

      const tabBounds = await page.getByTestId('nav-tabs').getByRole('link').evaluateAll((links) =>
        links.map((link) => {
          const box = link.getBoundingClientRect()
          return { left: box.left, right: box.right, width: box.width }
        }),
      )
      expect(tabBounds.length).toBe(4)
      expect(tabBounds.every(({ left, right, width }) => left >= -1 && right <= viewportWidth + 1 && width > 0)).toBe(true)

      const pageScroll = await page.evaluate(() => {
        window.scrollTo({ left: window.innerWidth, top: 0 })
        return window.scrollX
      })
      expect(pageScroll, 'page itself must not scroll horizontally').toBe(0)

      const tableScroll = await tableWrapper.evaluate((element) => ({
        scrollable: element.scrollWidth > element.clientWidth,
        clientWidth: element.clientWidth,
        scrollWidth: element.scrollWidth,
      }))
      if (width <= 375) {
        expect(tableScroll.scrollable, 'narrow People table should scroll inside its wrapper').toBe(true)
      }
      expect(tableScroll.scrollWidth).toBeGreaterThanOrEqual(tableScroll.clientWidth)
    })

    test('opens a seeded person from the table', async ({ page }) => {
      test.skip(width !== 768, 'navigation smoke runs once at tablet width')
      await signInAsOfficer(page)
      await page.goto('/officer/people')

      await page.getByTestId('people-row').first().click()
      await expect(page).toHaveURL(/\/officer\/people\/[0-9a-f-]+$/)
    })
  })
}
