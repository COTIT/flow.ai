import { expect, test, type Page } from '@playwright/test'

/**
 * The floating tab bar clears the system gesture area.
 *
 * jsdom has no safe area, so `SurfaceNav.test.tsx` asserts the mechanism. This
 * is the real thing: Chromium's DevTools protocol can impose a bottom inset, the
 * way an iPhone's home indicator or Android's gesture bar does, and the pill has
 * to move up by it. Read-only — it signs in and looks, and writes nothing.
 */
const PASSWORD = 'demo1234'

test.use({ viewport: { width: 390, height: 844 } })

async function setBottomInset(page: Page, bottom: number) {
  const session = await page.context().newCDPSession(page)
  await session.send('Emulation.setSafeAreaInsetsOverride', {
    insets: { top: 0, bottom, left: 0, right: 0 },
  })
}

async function signInAsOfficer(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('officer.ilundo@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/officer$/)
  await expect(page.getByTestId('nav-tabs')).toBeVisible()
}

/** Pixels between the pill's bottom edge and the bottom of the screen. */
async function gapBelowPill(page: Page) {
  return page.getByTestId('nav-tabs').evaluate((nav) => {
    const pill = nav.querySelector('ul')!.getBoundingClientRect()
    return window.innerHeight - pill.bottom
  })
}

test.describe('the tab bar and the bottom safe area', () => {
  test('with no inset the pill floats 12px off the edge', async ({ page }) => {
    await setBottomInset(page, 0)
    await signInAsOfficer(page)

    expect(await gapBelowPill(page)).toBeCloseTo(12, 0)
  })

  test('with a 34px home-indicator inset the pill rides up to clear it', async ({ page }) => {
    await setBottomInset(page, 34)
    await signInAsOfficer(page)

    // 34 - 10: the indicator sits in the gap beneath the pill, not on it.
    expect(await gapBelowPill(page)).toBeCloseTo(24, 0)
  })

  test('the published height covers the pill and the gap, and the content clears it', async ({
    page,
  }) => {
    await setBottomInset(page, 34)
    await signInAsOfficer(page)
    await page.goto('/officer/register')
    await expect(page.getByTestId('register-submit')).toBeVisible()

    const { published, pill, gap } = await page.getByTestId('nav-tabs').evaluate((nav) => ({
      published: parseFloat(
        document.documentElement.style.getPropertyValue('--tab-bar-height'),
      ),
      pill: nav.querySelector('ul')!.getBoundingClientRect().height,
      gap: window.innerHeight - nav.querySelector('ul')!.getBoundingClientRect().bottom,
    }))
    expect(published).toBeGreaterThanOrEqual(pill + gap - 1)

    // Scrolled as far as the page goes, a tap at the middle of the last control
    // reaches it, not the pill: the padding is what makes that true.
    const reaches = await page.getByTestId('register-submit').evaluate((el) => {
      window.scrollTo(0, document.documentElement.scrollHeight)
      const box = el.getBoundingClientRect()
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)
      return hit === el || el.contains(hit)
    })
    expect(reaches, 'the pill covers the register submit').toBe(true)
  })

  test('the strip around the pill does not swallow taps meant for the page', async ({ page }) => {
    await setBottomInset(page, 34)
    await signInAsOfficer(page)

    // Left of the pill, in the gutter: the strip is transparent and click-through.
    const target = await page.getByTestId('nav-tabs').evaluate((nav) => {
      const pill = nav.querySelector('ul')!.getBoundingClientRect()
      const hit = document.elementFromPoint(4, pill.top + pill.height / 2)
      return { strip: hit === nav, inside: nav.contains(hit) }
    })
    expect(target.strip, 'the transparent strip took the hit').toBe(false)
  })
})
