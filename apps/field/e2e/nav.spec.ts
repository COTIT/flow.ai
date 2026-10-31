import { expect, test, type Page } from '@playwright/test'

import { tr } from './support/i18n'

/**
 * THE GATE for M1–M4: every role's landing page and every nav destination
 * renders a real screen rather than a scaffold placeholder.
 *
 * Why this file exists. Eleven routes shipped as `Placeholder` and sat behind a
 * fully green 103-test suite, because every other spec navigates straight to a
 * deep route where the behaviour under test lives — `/officer/people/<id>`,
 * `/ops/requests`, `/farm/my-farm`. Nothing ever visited the page a human
 * actually lands on after signing in. QA-FINDINGS.md #15.
 *
 * Scope, against its neighbour. `shell.spec.ts` already asserts nav STRUCTURE
 * (farmer and officer get tabs, ops gets a sidebar), the link labels, and that
 * ops sidebar links and farmer tabs resolve when clicked. This file does not
 * repeat any of that. It asserts what is BEHIND each link, plus the two things
 * shell.spec leaves out: the landing screens, and that the officer's tabs
 * resolve.
 *
 * All eleven placeholders were replaced across M1–M4, so every destination is
 * now asserted the same way: the real screen renders, and no placeholder text
 * appears anywhere in it. While the screens were being built this file carried
 * a per-route `built` flag, pinning the not-yet-built ones as characterisation
 * tests so the suite stayed green and other breakage still surfaced. That
 * scaffolding is gone now that it reads true everywhere.
 */
const PASSWORD = 'demo1234'
const PLACEHOLDER = /Session 1 placeholder/

interface Destination {
  /** Path to visit. */
  path: string
  /** `data-testid` that proves the real screen rendered. */
  ready: string
}

interface Surface {
  role: string
  email: string
  /** Where signing in lands, and that landing screen's own expectations. */
  home: Destination
  nav: Destination[]
  /** Spec 4.1: farmer and officer get tabs, ops gets a sidebar. */
  navTestId: 'nav-tabs' | 'nav-sidebar'
}

const SURFACES: Surface[] = [
  {
    role: 'farmer',
    email: 'neema@demo.ruaha360.test',
    navTestId: 'nav-tabs',
    // §6.1 — T2.
    home: { path: '/farm', ready: 'farm-home' },
    nav: [
      { path: '/farm/my-farm', ready: 'my-farm' },
      { path: '/farm/equipment', ready: 'equipment-list' },
      { path: '/farm/requests', ready: 'requests-list' },
      // §6.6 — T2. Every placeholder is now gone.
      { path: '/farm/opportunities', ready: 'farmer-opportunities' },
    ],
  },
  {
    role: 'field_officer',
    email: 'officer.ilundo@demo.ruaha360.test',
    navTestId: 'nav-tabs',
    // §5.1 — T1.
    home: { path: '/officer', ready: 'officer-home' },
    nav: [
      { path: '/officer/register', ready: 'register-submit' },
      // §5.3 — T2.
      { path: '/officer/people', ready: 'people-table' },
      // §5.7 — T2.
      { path: '/officer/verify', ready: 'verify-queue' },
    ],
  },
  {
    role: 'ops',
    email: 'ops@demo.ruaha360.test',
    navTestId: 'nav-sidebar',
    // §7.1 — T2.
    home: { path: '/ops', ready: 'ops-home' },
    nav: [
      { path: '/ops/requests', ready: 'requests-table' },
      { path: '/ops/demand', ready: 'demand-table' },
      // §7.4 — T1 read; editing stays T2
      { path: '/ops/catalogue', ready: 'catalogue-table' },
      // §7.5 — T2.
      { path: '/ops/buyers', ready: 'buyers-table' },
      // §7.9 — T2.
      { path: '/ops/villages', ready: 'villages-table' },
      { path: '/ops/tower', ready: 'tower' },
    ],
  },
]

/** Admin holds the ops surface — `SURFACE_ROLES.ops` includes admin. */
const ADMIN = 'admin@demo.ruaha360.test'

async function signIn(page: Page, email: string, home: RegExp) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  // Wait for the landing redirect: navigating before it lands races the
  // session and drops straight back to /login.
  await expect(page).toHaveURL(home)
}

/** A destination renders its real screen, cleanly, with no scaffolding left. */
async function assertDestination(page: Page, dest: Destination) {
  await expect(page.getByTestId(dest.ready)).toBeVisible()
  await expect(page.getByTestId('error-state')).toHaveCount(0)
  await expect(page.locator('main')).not.toContainText(PLACEHOLDER)
}

for (const surface of SURFACES) {
  test.describe(`${surface.role} surface`, () => {
    test(`lands on ${surface.home.path} with a real screen`, async ({ page }) => {
      await signIn(page, surface.email, new RegExp(`${surface.home.path}$`))
      await assertDestination(page, surface.home)
    })

    // One test per destination, so a failure names the screen rather than
    // "the officer surface".
    for (const dest of surface.nav) {
      test(`nav destination ${dest.path}`, async ({ page }) => {
        await signIn(page, surface.email, new RegExp(`${surface.home.path}$`))
        await page.goto(dest.path)
        await assertDestination(page, dest)
      })
    }
  })
}

test.describe('gaps shell.spec.ts does not cover', () => {
  // shell.spec asserts the officer's tab LABELS but never clicks them; it
  // covers click-resolution for ops and farmer only.
  test('every officer tab resolves when clicked', async ({ page }) => {
    await signIn(page, 'officer.ilundo@demo.ruaha360.test', /\/officer$/)
    const tabs = page.getByTestId('nav-tabs')

    for (const [label, path] of [
      [tr('sw', 'nav.register'), '/officer/register'],
      [tr('sw', 'nav.people'), '/officer/people'],
      [tr('sw', 'nav.verify'), '/officer/verify'],
    ] as const) {
      await tabs.getByRole('link', { name: label }).click()
      // Register adds ?draft=<id> on arrival, so the path may carry a query.
      await expect(page).toHaveURL(new RegExp(`${path}(\\?|$)`))
    }
  })

  test('admin lands on /ops and gets the ops sidebar', async ({ page }) => {
    await signIn(page, ADMIN, /\/ops$/)
    await expect(page.getByTestId('nav-sidebar')).toBeVisible()
    await expect(page.getByTestId('nav-tabs')).toHaveCount(0)
  })
})
