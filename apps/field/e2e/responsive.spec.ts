import { expect, test, type Page } from '@playwright/test'

import { CROP } from './support/seed'

/**
 * QA #8 — the ops surface below ~800px.
 *
 * The sidebar kept its full width and never collapsed, so the content column
 * was squeezed and clipped at the right edge: on the Tower "12,000" and
 * "1,150" were cut mid-figure, and on the request pipeline the Village column
 * and the title were cut. The table's own `overflow-x: auto` wrapper works, so
 * the page did not scroll sideways — it just hid data.
 *
 * `docs/screens-and-components.md` allows ops to be desktop-first, and this
 * does not make it mobile-first. It makes truncation stop being silent.
 */
const PASSWORD = 'demo1234'
const ILUNDO = '30000000-0000-4000-8000-000000000001'

async function signInAsOps(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/ops$/)
}

/** Nothing inside `locator` may extend past the right edge of the viewport. */
async function assertNotClipped(page: Page, testId: string) {
  const box = await page.getByTestId(testId).boundingBox()
  const width = page.viewportSize()!.width
  expect(box, `${testId} should be laid out`).not.toBeNull()
  expect(box!.x + box!.width, `${testId} is cut off at ${width}px`).toBeLessThanOrEqual(width)
}

test.describe('the ops surface on a phone', () => {
  test.use({ viewport: { width: 375, height: 812 } })

  test('the Tower figures are not cut off at the right edge', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/tower?village=${ILUNDO}`)
    await expect(page.getByTestId('tile-production')).toBeVisible()

    await assertNotClipped(page, 'tile-production')
    await assertNotClipped(page, 'tile-energy')
  })

  /**
   * A wide table SHOULD be wider than a phone. The finding is explicit that
   * the table's own `overflow-x: auto` wrapper works — what failed was the
   * page around it. So the claim here is that the wrapper fits the viewport
   * and the table scrolls INSIDE it, which is the difference between data
   * being reachable and being hidden.
   */
  test('the request pipeline scrolls its columns rather than hiding them', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/requests')
    await expect(page.getByTestId('requests-table')).toBeVisible()

    const wrapper = page.getByTestId('requests-table').locator('xpath=..')
    const box = await wrapper.boundingBox()
    expect(box!.x + box!.width).toBeLessThanOrEqual(page.viewportSize()!.width)

    const scrollable = await wrapper.evaluate((el) => el.scrollWidth > el.clientWidth)
    expect(scrollable, 'the table should scroll inside its own wrapper').toBe(true)

    // And scrolling it actually reaches the far column.
    await wrapper.evaluate((el) => el.scrollTo({ left: el.scrollWidth }))
    expect(await wrapper.evaluate((el) => el.scrollLeft)).toBeGreaterThan(0)
  })

  // The nav still has to be there — collapsing is not hiding.
  test('every nav destination is still reachable', async ({ page }) => {
    await signInAsOps(page)

    const nav = page.getByTestId('nav-sidebar')
    await expect(nav).toBeVisible()
    for (const label of ['Requests', 'Demand', 'Tower']) {
      await expect(nav.getByRole('link', { name: label })).toBeVisible()
    }
  })

  test('and the page does not scroll sideways', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/tower?village=${ILUNDO}`)
    await expect(page.getByTestId('tile-production')).toBeVisible()

    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    )
    expect(overflow).toBeLessThanOrEqual(1)
  })

  test('page headers and shell actions use a stacked mobile composition', async ({ page }) => {
    await signInAsOps(page)

    for (const path of ['/ops', '/ops/requests', '/ops/demand', '/ops/buyers', '/ops/tower']) {
      await page.goto(path)
      await expect(page.getByTestId('page-header')).toBeVisible()

      const viewportWidth = page.viewportSize()!.width
      const pageHeader = await page.getByTestId('page-header').boundingBox()
      expect(pageHeader!.x + pageHeader!.width).toBeLessThanOrEqual(viewportWidth)

      // On a phone the shell's action row folds into one menu trigger (the
      // user menu); the full row is for `sm` and up.
      await expect(page.getByTestId('global-header-actions')).toBeHidden()
      const trigger = await page.getByTestId('user-menu-trigger').boundingBox()
      expect(trigger!.x + trigger!.width).toBeLessThanOrEqual(viewportWidth)
      expect(trigger!.height).toBeGreaterThanOrEqual(44)

      const pageActions = page.getByTestId('page-header-actions')
      if (await pageActions.count()) {
        const actionBox = await pageActions.boundingBox()
        expect(actionBox!.x + actionBox!.width).toBeLessThanOrEqual(viewportWidth)
        expect(actionBox!.width).toBeGreaterThanOrEqual(pageHeader!.width * 0.9)

        const controls = await pageActions.locator('button, [role="button"]').evaluateAll((elements) =>
          elements.map((element) => {
            const box = element.getBoundingClientRect()
            return { right: box.right, height: box.height }
          }),
        )
        expect(controls.length).toBeGreaterThan(0)
        expect(controls.every(({ right, height }) => right <= viewportWidth && height >= 44)).toBe(true)
      }
    }
  })
})

test.describe('the ops surface on a desktop', () => {
  test.use({ viewport: { width: 1280, height: 900 } })

  // Desktop-first is the spec's choice for ops and stays the choice.
  test('still gets the sidebar beside the content', async ({ page }) => {
    await signInAsOps(page)

    const nav = await page.getByTestId('nav-sidebar').boundingBox()
    const main = await page.locator('main').boundingBox()

    expect(nav!.x + nav!.width).toBeLessThanOrEqual(main!.x + 1)
  })
})

/**
 * The wide-screen pass protects against the opposite failure: a screen that
 * technically fits but leaves its real workspace stranded in a narrow island.
 * These assertions deliberately measure rendered geometry rather than class
 * names, so a future wrapper cannot reintroduce the screenshot problem by
 * changing one utility class.
 */
test.describe('the workspace uses wide screens deliberately', () => {
  test.use({ viewport: { width: 1440, height: 900 } })

  async function assertUsesWorkspace(page: Page, testId: string) {
    const box = await page.getByTestId(testId).boundingBox()
    expect(box, `${testId} should be laid out`).not.toBeNull()
    expect(box!.width, `${testId} is stranded in a narrow column`).toBeGreaterThan(1440 * 0.72)
  }

  test('demand keeps creation progressive and expands the form when opened', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/demand')

    await expect(page.getByTestId('demand-create-panel')).toHaveCount(0)
    await assertUsesWorkspace(page, 'demand-table')

    await page.getByTestId('demand-create-open').click()
    await expect(page.getByTestId('demand-create-panel')).toBeVisible()
    await assertUsesWorkspace(page, 'demand-create-panel')
  })

  test('buyers keeps creation progressive and expands the form when opened', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/buyers')

    await expect(page.getByTestId('buyer-create-panel')).toHaveCount(0)
    await assertUsesWorkspace(page, 'buyers-table')

    await page.getByTestId('buyer-create-open').click()
    await expect(page.getByTestId('buyer-create-panel')).toBeVisible()
    await assertUsesWorkspace(page, 'buyer-create-panel')
  })

  test('request detail uses the available desktop workspace', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/requests/d0000000-0000-4000-8000-000000000005')
    await expect(page.getByTestId('request-review')).toBeVisible()
    await assertUsesWorkspace(page, 'request-review')
  })

  test('Tower drills use the available desktop workspace', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/tower/production?village=${ILUNDO}`)
    await expect(page.getByTestId('production-table')).toBeVisible()
    await assertUsesWorkspace(page, 'production-table')

    await page.goto(`/ops/tower/market?village=${ILUNDO}`)
    await expect(page.getByTestId('market-table')).toBeVisible()
    await assertUsesWorkspace(page, 'market-table')
  })

  test('officer record detail uses the available desktop workspace', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('officer.ilundo@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/officer$/)

    await page.goto('/officer/people/60000000-0000-4000-8000-000000000001')
    await expect(page.getByTestId('person-detail')).toBeVisible()
    await assertUsesWorkspace(page, 'person-detail')
  })
})

for (const width of [320, 375, 768, 1024, 1440] as const) {
  test.describe(`workspace frame at ${width}px`, () => {
    test.use({ viewport: { width, height: 900 } })

    test('keeps page overflow out of the viewport and table overflow inside the table', async ({ page }) => {
      await signInAsOps(page)
      await page.goto('/ops/demand')
      await expect(page.getByTestId('demand-table')).toBeVisible()

      const viewportWidth = page.viewportSize()!.width
      const tableWrapper = page.getByTestId('demand-table').locator('xpath=..')
      const wrapperBox = await tableWrapper.boundingBox()
      expect(wrapperBox!.x + wrapperBox!.width).toBeLessThanOrEqual(viewportWidth)

      const documentOverflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      )
      expect(documentOverflow).toBeLessThanOrEqual(1)

      await page.getByTestId('demand-create-open').click()
      const panelBox = await page.getByTestId('demand-create-panel').boundingBox()
      expect(panelBox!.x + panelBox!.width).toBeLessThanOrEqual(viewportWidth)
    })
  })
}

/**
 * The other half of the tab-bar problem, and the half no unit test can see.
 *
 * `RootLayout` pads `main` clear of the fixed bar, which is enough for content
 * that flows. The register submit bar must flow too: when it was sticky it
 * covered the final confidence choice on a phone. The same padding then keeps
 * both the final field and Register button clear of the fixed tab bar.
 *
 * So the claim is the one that matters to a thumb — a tap at the middle of the
 * control reaches the control — rather than anything about classes.
 */
test.describe('the officer surface on a phone', () => {
  test.use({ viewport: { width: 375, height: 812 } })

  async function signInAsOfficer(page: Page) {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('officer.ilundo@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/officer$/)
  }

  /** What is actually at the middle of this control: it, or something over it. */
  async function tapReaches(page: Page, testId: string) {
    return page.getByTestId(testId).evaluate((el) => {
      el.scrollIntoView({ block: 'center' })
      const box = el.getBoundingClientRect()
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)
      return hit === el || el.contains(hit)
    })
  }

  test('the tab bar does not cover the register submit', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')
    await expect(page.getByTestId('register-submit')).toBeVisible()

    expect(
      await tapReaches(page, 'register-submit'),
      'the submit bar is behind the fixed tab bar',
    ).toBe(true)
  })

  test('the submit bar does not cover the last confidence choice', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const confidence = page.getByTestId('register-confidence').locator('label').last()
    await confidence.scrollIntoViewIfNeeded()

    const [choice, submitBar, viewport] = await Promise.all([
      confidence.boundingBox(),
      page.getByTestId('register-submit-bar').boundingBox(),
      page.evaluate(() => window.innerHeight),
    ])

    expect(choice).not.toBeNull()
    expect(submitBar).not.toBeNull()
    expect(
      submitBar!.y >= choice!.y + choice!.height || submitBar!.y >= viewport,
      'the submit bar covers the final confidence choice',
    ).toBe(true)
  })

  test('the harvest dates stack in full-width rows inside their card', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')
    // The window belongs to a crop, so it appears once one is ticked.
    await page.getByTestId(`register-crop-${CROP.MAIZE.id}`).check()

    const start = page.getByTestId('register-harvest-start')
    const end = page.getByTestId('register-harvest-end')
    await start.scrollIntoViewIfNeeded()
    const [a, b, card] = await Promise.all([
      start.boundingBox(),
      end.boundingBox(),
      page.getByTestId('register-group-cycle').boundingBox(),
    ])

    expect(a!.y + a!.height).toBeLessThanOrEqual(b!.y)
    expect(Math.abs(a!.width - b!.width)).toBeLessThanOrEqual(1)
    expect(a!.x).toBeGreaterThanOrEqual(card!.x)
    expect(a!.x + a!.width).toBeLessThanOrEqual(card!.x + card!.width)
    // Wider than the half-width column an iOS date field could not fill.
    expect(a!.width).toBeGreaterThan(250)
  })

  test('the white submit bar runs to the bottom of the screen, with no strip of page colour', async ({
    page,
  }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')
    await expect(page.getByTestId('register-submit')).toBeVisible()
    await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight))

    const [bar, viewport, submit, nav] = await Promise.all([
      page.getByTestId('register-submit-bar').boundingBox(),
      page.evaluate(() => window.innerHeight),
      page.getByTestId('register-submit').boundingBox(),
      page.getByTestId('nav-tabs').boundingBox(),
    ])
    expect(Math.abs(bar!.y + bar!.height - viewport)).toBeLessThanOrEqual(1)
    // and the button still clears the tab bar.
    expect(submit!.y + submit!.height).toBeLessThanOrEqual(nav!.y)
  })

  test('the farm position stays editable and can be re-read from the phone GPS', async ({
    page,
  }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const latitude = page.getByTestId('register-farm-latitude')
    await expect(latitude).toHaveValue(/^-8\.13/)
    await expect(latitude).toBeEnabled()
    await latitude.fill('1.5')
    await page.getByTestId('register-gps-locate').click()
    await expect(latitude).toHaveValue(/^-8\.13/)
    await expect(page.getByTestId('register-farm-longitude')).toHaveValue(/^35\.18/)
  })

  test('the provenance note is one line', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')

    const note = page.getByTestId('register-provenance-note')
    await expect(note).toBeVisible()
    const lineHeight = await note.evaluate((el) => parseFloat(getComputedStyle(el).lineHeight))
    const box = await note.boundingBox()
    expect(box!.height).toBeLessThan(lineHeight * 2)
  })

  test('and the register page does not scroll sideways', async ({ page }) => {
    await signInAsOfficer(page)
    await page.goto('/officer/register')
    await expect(page.getByTestId('register-submit')).toBeVisible()

    const overflow = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    )
    expect(overflow).toBeLessThanOrEqual(1)
  })
})
