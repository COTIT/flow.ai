import { expect, test, type Page } from '@playwright/test'

import { escapeRe, tr } from './support/i18n'

/**
 * Spec 4.1: "Role-aware nav. Farmer and Officer get a bottom tab bar; Ops gets
 * a sidebar." Farmer and officer surfaces are mobile-first, ops desktop-first.
 */
const PASSWORD = 'demo1234'

// Ops is an 'en' session; farmers and officers are 'sw' (see support/i18n.ts).
const OPS_NAV = [
  'nav.requests',
  'nav.demand',
  'nav.catalogue',
  'nav.buyers',
  'nav.villages',
  'nav.surveys',
  'nav.tower',
]

async function signIn(page: Page, email: string) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
}

test.describe('app shell', () => {
  test('a farmer gets a bottom tab bar and no sidebar', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/farm$/)

    const tabs = page.getByTestId('nav-tabs')
    await expect(tabs).toBeVisible()
    await expect(page.getByTestId('nav-sidebar')).toHaveCount(0)

    // Surveys carries the in-app count of surveys waiting, read out with its
    // plural (surveys.badge, on count). Neema is a 'sw' user, so the label is
    // Swahili and its plural form has to follow the number.
    //
    // The count is read from the badge and not pinned to the seed's two. Two
    // surveys wait for Neema in a fresh seed, but answering one is exactly what
    // the demo does, and that leaves a voucher on an append-only trail that a
    // test has no business undoing. What is asserted is that a survey IS
    // waiting and that the label agrees with the number beside it.
    const badge = tabs.getByTestId('nav-badge-surveys')
    await expect(badge).toBeVisible()
    // The badge carries the number and, for a screen reader, its label: the
    // number is the first run of digits.
    const waiting = Number(/\d+/.exec((await badge.textContent()) ?? '')?.[0])
    expect(waiting).toBeGreaterThan(0)

    await expect(tabs.getByRole('link')).toHaveText([
      tr('sw', 'nav.myFarm'),
      tr('sw', 'nav.equipment'),
      tr('sw', 'nav.requests'),
      tr('sw', 'nav.opportunities'),
      new RegExp(
        `^${escapeRe(tr('sw', 'nav.surveys'))}${waiting}${escapeRe(tr('sw', 'surveys.badge', { count: waiting }))}$`,
      ),
    ])
  })

  test('an officer gets a bottom tab bar', async ({ page }) => {
    await signIn(page, 'officer.ilundo@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/officer$/)

    const tabs = page.getByTestId('nav-tabs')
    await expect(tabs).toBeVisible()
    await expect(page.getByTestId('nav-sidebar')).toHaveCount(0)
    await expect(tabs.getByRole('link')).toHaveText([
      tr('sw', 'nav.register'),
      tr('sw', 'nav.people'),
      tr('sw', 'nav.verify'),
      tr('sw', 'nav.redeem'),
    ])
  })

  test('ops gets a sidebar and no tab bar, Tower included', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/ops$/)

    const sidebar = page.getByTestId('nav-sidebar')
    await expect(sidebar).toBeVisible()
    await expect(page.getByTestId('nav-tabs')).toHaveCount(0)

    await expect(sidebar.getByRole('link')).toHaveText(OPS_NAV.map((key) => tr('en', key)))
  })

  test('the nav is navigable: every ops sidebar link resolves', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/ops$/)

    for (const label of OPS_NAV.map((key) => tr('en', key))) {
      await page.getByTestId('nav-sidebar').getByRole('link', { name: label }).click()
      // A route that does not resolve renders the router's error or not-found
      // state, neither of which has a heading, so any heading proves the
      // target exists. Deliberately level-agnostic: asserting h2 tied this to
      // the placeholder markup and broke as real screens replaced them.
      await expect(page.getByRole('heading').first()).toBeVisible()
    }
  })

  test('every farmer tab resolves', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/farm$/)

    const labels = [
      tr('sw', 'nav.myFarm'),
      tr('sw', 'nav.equipment'),
      tr('sw', 'nav.requests'),
      tr('sw', 'nav.opportunities'),
      new RegExp(`^${escapeRe(tr('sw', 'nav.surveys'))}`),
    ]
    for (const label of labels) {
      await page.getByTestId('nav-tabs').getByRole('link', { name: label }).click()
      await expect(page.getByRole('heading').first()).toBeVisible()
    }
  })

  test('signed out there is no surface nav at all', async ({ page }) => {
    await page.goto('/login')
    await expect(page.getByTestId('login-submit')).toBeVisible()
    await expect(page.getByTestId('nav-tabs')).toHaveCount(0)
    await expect(page.getByTestId('nav-sidebar')).toHaveCount(0)
  })

  test('the demo banner is present on every surface', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test')
    await expect(page.getByTestId('demo-banner')).toBeVisible()
    await page.goto('/ops/tower')
    await expect(page.getByTestId('demo-banner')).toBeVisible()
  })
})

/**
 * Every tab label is one line under its icon, in both languages, on the
 * smallest phone the demo targets (375px). Five tabs leave about 59px of text
 * at 12px. A single word never wraps there (it overflows its tab instead), but
 * a label with a space does the moment it is wider than the tab: "Equipment &
 * loans" and "Shamba langu" both did (19 Oct 2026), which made the bar uneven.
 * The rendered bar is checked by height; the other language is measured in the
 * tab's own font, since switching the language would write app_user.locale.
 */
test.describe('tab labels stay on one line at phone width', () => {
  test.use({ viewport: { width: 375, height: 740 } })

  const TAB_NAV = {
    farmer: ['nav.myFarm', 'nav.equipment', 'nav.requests', 'nav.opportunities', 'nav.surveys'],
    officer: ['nav.register', 'nav.people', 'nav.verify', 'nav.redeem'],
  } as const

  async function expectOneLineTabs(page: Page, keys: readonly string[]) {
    const tabs = page.getByTestId('nav-tabs')
    await expect(tabs).toBeVisible()

    // Rendered: no tab taller than its neighbours.
    const heights = await tabs.locator('a').evaluateAll((links) =>
      links.map((link) => Math.round(link.getBoundingClientRect().height)),
    )
    expect(heights.every((height) => height === heights[0]), heights.join(', ')).toBe(true)

    // Both languages, measured in the font the tab renders with. A label that
    // contains a space must fit the tab's content box or it wraps there.
    const tooWide = await tabs.locator('a').first().evaluate((link, labels) => {
      const style = getComputedStyle(link)
      const available =
        link.getBoundingClientRect().width -
        parseFloat(style.paddingLeft) -
        parseFloat(style.paddingRight)
      const canvas = document.createElement('canvas').getContext('2d')!
      // The active tab is semibold; measure every label at its widest.
      canvas.font = `600 ${style.fontSize} ${style.fontFamily}`
      return labels
        .map((label) => ({ label, width: Math.round(canvas.measureText(label).width) }))
        .filter(({ label, width }) => label.includes(' ') && width > available)
    }, keys.flatMap((key) => [tr('en', key), tr('sw', key)]))
    expect(tooWide).toEqual([])
  }

  test('farmer', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/farm$/)
    await expectOneLineTabs(page, TAB_NAV.farmer)
  })

  test('officer', async ({ page }) => {
    await signIn(page, 'officer.ilundo@demo.ruaha360.test')
    await expect(page).toHaveURL(/\/officer$/)
    await expectOneLineTabs(page, TAB_NAV.officer)
  })
})
