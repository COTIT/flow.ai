import { expect, test, type Page } from '@playwright/test'

import { VILLAGE } from './support/seed'
import { assertsSeededFigures } from './support/seeded'
import { chooseSelect } from './support/select'

/**
 * Spec §8 — the Control Tower, built last, from connected records.
 *
 * Every figure asserted here was read out of the views first, so these are
 * the database's numbers rather than my arithmetic. CLAUDE.md's headline set
 * appears in full: 12,000 / 6,400 / 5,600 / 62.2% / 10.800 / 489.200.
 *
 * 8.2's acceptance: "from any Tower headline, reach a single farmer's record
 * in at most three clicks. If a number cannot be traced, it does not belong on
 * the screen."
 */

const PASSWORD = 'demo1234'

// This spec asserts seeded figures, so it starts from seeded state.
assertsSeededFigures()

async function signInAsOps(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/ops$/)
}

async function openIlundoTower(page: Page) {
  await signInAsOps(page)
  await page.goto(`/ops/tower?village=${VILLAGE.ILUNDO}`)
  await expect(page.getByTestId('tower')).toBeVisible()
}

/**
 * Opens a tile's drill-down, after its figure has actually arrived.
 *
 * QA #29: the drill is deliberately NOT a link while the tile is still
 * loading — clicking one before the headline resolved landed on a drill-down
 * whose own query had not started, from a figure nobody had seen. So the wait
 * here is the behaviour under test, not a workaround for it.
 */
async function openDrill(page: Page, tile: string) {
  const section = page.getByTestId(tile)
  await expect(section).toBeVisible()
  await expect(section).not.toHaveAttribute('aria-busy', 'true')
  await section.getByTestId('tile-drill').click()
}

test.describe('/ops/tower overview', () => {
  test('the village selector is held in the URL', async ({ page }) => {
    await openIlundoTower(page)
    await expect(page.getByTestId('tower-village')).toContainText('Ilundo')

    await chooseSelect(page, 'tower-village', 'Mgama')
    await expect(page).toHaveURL(new RegExp(`village=${VILLAGE.MGAMA}`))
  })

  test('a nonsense village in the URL does not break the screen', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/tower?village=not-a-uuid')
    await expect(page.getByTestId('tower')).toBeVisible()
    await expect(page.getByTestId('error-state')).toHaveCount(0)
  })

  test('all five tiles are present', async ({ page }) => {
    await openIlundoTower(page)
    for (const tile of ['production', 'pue', 'energy', 'market', 'quality']) {
      await expect(page.getByTestId(`tile-${tile}`)).toBeVisible()
    }
  })

  test('production shows the September maize expected total', async ({ page }) => {
    await openIlundoTower(page)
    await expect(page.getByTestId('tile-production')).toContainText('12,000.00 kg')
  })

  // cycle_area_ha is "planted area across cycles", never "land area":
  // intercropping means it can exceed the village's hectares.
  test('planted area is labelled as area across cycles, not land area', async ({ page }) => {
    await openIlundoTower(page)
    const tile = page.getByTestId('tile-production')
    await expect(tile).toContainText('4.9000 ha')
    await expect(tile).toContainText(/planted area across cycles/i)
    // The tile's own note disclaims the wrong reading ("It is not land area"),
    // so a bare /land area/ search matches the correct copy. What must not
    // appear is the figure being LABELLED as land area.
    await expect(tile).toContainText(/it is not land area/i)
    await expect(tile).not.toContainText(/hectares farmed/i)
    await expect(tile).not.toContainText(/^land area/im)
  })

  test('the PUE pipeline shows counts and indicative value by status', async ({ page }) => {
    await openIlundoTower(page)
    const tile = page.getByTestId('tile-pue')
    await expect(tile).toContainText('Approved')
    await expect(tile).toContainText('TZS 38,600,000.00')
    await expect(tile).toContainText(/indicative/i)
  })

  test('energy shows capacity with its basis, and never as measured', async ({ page }) => {
    await openIlundoTower(page)
    const tile = page.getByTestId('tile-energy')
    await expect(tile).toContainText('500.000 kW')
    await expect(tile).toContainText(/planned capacity/i)
    await expect(tile).toContainText(/basis: planned/i)
    // The approved-peak note reads "Still not measured consumption", which is
    // required copy, so a bare /measured/ search matches the right thing. What
    // must never appear is a claim that a figure IS measured.
    await expect(tile).not.toContainText(/measured capacity/i)
    await expect(tile).not.toContainText(/measured consumption:/i)
  })

  // Prospective and approved are separate figures, never summed.
  test('prospective and approved peaks are separate, and stated as such', async ({ page }) => {
    await openIlundoTower(page)
    const tile = page.getByTestId('tile-energy')
    await expect(page.getByTestId('tower-prospective-peak')).toHaveText('7.200 kW')
    await expect(page.getByTestId('tower-approved-peak')).toHaveText('10.800 kW')
    await expect(page.getByTestId('tower-headroom')).toHaveText('489.200 kW')
    await expect(tile).toContainText(/never added together|separate figures/i)
    // 12.000 + 18.000 = 30.000 raw, and 7.200 + 10.800 = 18.000 corrected.
    // Neither combined figure may appear.
    await expect(tile).not.toContainText('30.000 kW')
    await expect(tile).not.toContainText('18.000 kW')
  })

  // The factor is displayed next to the peak it was applied to.
  test('the simultaneity factor sits with the peak it was applied to', async ({ page }) => {
    await openIlundoTower(page)
    await expect(page.getByTestId('tower-simultaneity')).toContainText('0.6')
    await expect(page.getByTestId('tile-energy')).toContainText(/simultaneity/i)
  })

  test('market shows open demand against available supply', async ({ page }) => {
    await openIlundoTower(page)
    const tile = page.getByTestId('tile-market')
    await expect(tile).toContainText('9,000.00 kg')
    await expect(tile).toContainText('5,600.00 kg')
    await expect(tile).toContainText('62.2%')
  })

  test('data quality shows verified share, GPS coverage and cycles with estimates', async ({
    page,
  }) => {
    await openIlundoTower(page)
    const tile = page.getByTestId('tile-quality')
    // Ilundo: 3 of 6 persons verified, 3 of 4 farms with GPS, 5 of 7 cycles
    // with a current expected figure.
    await expect(tile).toContainText('3 / 6')
    await expect(tile).toContainText('3 / 4')
    await expect(tile).toContainText('5 / 7')
  })

  test('a farmer sees no Tower at all', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/farm$/)

    await page.goto('/ops/tower')
    await expect(page).toHaveURL(/\/farm$/)
  })
})

test.describe('/ops/tower drill-downs', () => {
  test('production drills to rows that end in a crop cycle', async ({ page }) => {
    await openIlundoTower(page)
    await openDrill(page, 'tile-production')

    await expect(page).toHaveURL(/\/ops\/tower\/production/)
    await expect(page.getByTestId('production-table')).toBeVisible()
    await expect(page.getByTestId('production-table')).toBeVisible()
    // The grouped headline and the rows behind it are on the same screen.
    await expect(page.getByText('12,000.00 kg').first()).toBeVisible()
    // Every row ends in a link to an actual crop cycle.
    await expect(
      page.getByTestId('production-row').first().getByTestId('production-cycle').getByTestId('drill-link'),
    ).toBeVisible()
  })

  // QA-FINDINGS #1: /officer/cycles/$cycleId was a placeholder, so spec §8.2's
  // "reach a single farmer's record" dead-ended here. M2 built the screen.
  test('the production drill reaches a real crop cycle, not a placeholder', async ({ page }) => {
    await openIlundoTower(page)
    await openDrill(page, 'tile-production')
    await expect(page.getByTestId('production-table')).toBeVisible()

    await page
      .getByTestId('production-row')
      .first()
      .getByTestId('production-cycle')
      .getByTestId('drill-link')
      .click()

    await expect(page.getByTestId('cycle-detail')).toBeVisible()
    await expect(page.locator('main')).not.toContainText('Session 1 placeholder')
    // A harvest figure is a series: the current row and anything it replaced.
    expect(await page.getByTestId('cycle-harvest').count()).toBeGreaterThan(0)
  })

  test('energy drills to rows that end in a request', async ({ page }) => {
    await openIlundoTower(page)
    await openDrill(page, 'tile-energy')

    await expect(page).toHaveURL(/\/ops\/tower\/energy/)
    await expect(page.getByTestId('energy-table')).toBeVisible()
    await expect(page.getByTestId('energy-row').first().getByTestId('drill-link')).toBeVisible()
  })

  test('market drills to rows that end in an opportunity', async ({ page }) => {
    await openIlundoTower(page)
    await openDrill(page, 'tile-market')

    await expect(page).toHaveURL(/\/ops\/tower\/market/)
    await expect(page.getByTestId('market-table')).toBeVisible()
    await expect(page.getByTestId('market-table')).toContainText('62.2%')
  })

  // 8.2's acceptance criterion, counted literally.
  test('a Tower headline reaches a farmer record in three clicks', async ({ page }) => {
    await openIlundoTower(page)

    // 1
    await openDrill(page, 'tile-market')
    await expect(page.getByTestId('market-table')).toBeVisible()

    // 2 — the opportunity, not the buyer: the row drills both ways.
    await page
      .getByTestId('market-row')
      .first()
      .getByTestId('market-opportunity')
      .getByTestId('drill-link')
      .click()
    await expect(page.getByTestId('opportunity-detail')).toBeVisible()

    // 3
    await page.getByTestId('supply-row').first().getByTestId('drill-link').first().click()
    await expect(page.getByTestId('person-detail')).toBeVisible()
    await expect(page.getByTestId('person-detail')).toContainText('Neema Mwakalinga')
  })

  // QA-FINDINGS.md #7. The drill listed all six statuses under "requests
  // behind the energy figures" while the headlines counted three of them, so
  // the column added to 42.5 kW against a screen showing 7.200 and 10.800.
  test('the energy drill reconciles with the headlines it came from', async ({ page }) => {
    await openIlundoTower(page)
    await openDrill(page, 'tile-energy')
    await expect(page.getByTestId('energy-table')).toBeVisible()

    // Each group states its own arithmetic and lands on the tile's figure.
    await expect(page.getByTestId('energy-prospective-peak')).toHaveText('7.200 kW')
    await expect(page.getByTestId('energy-approved-peak')).toHaveText('10.800 kW')

    // Seeded Ilundo: the grain dryer alone is under review (12.000 kW raw),
    // and the mill plus the pump are approved (15.000 + 3.000 = 18.000 raw).
    const prospective = page.getByTestId('energy-group-prospective')
    await expect(prospective).toContainText('12.000 kW')
    await expect(prospective.getByTestId('energy-row')).toHaveCount(1)

    const approved = page.getByTestId('energy-group-approved')
    await expect(approved).toContainText('18.000 kW')
    await expect(approved.getByTestId('energy-row')).toHaveCount(2)

    // The factor is shown with the peak it was applied to, not elsewhere.
    await expect(prospective).toContainText('0.6')
  })

  test('the energy drill excludes rows that feed neither figure, and says so', async ({ page }) => {
    await openIlundoTower(page)
    await openDrill(page, 'tile-energy')
    await expect(page.getByTestId('energy-table')).toBeVisible()

    // Seeded Ilundo also holds a draft cold room and a rejected oil press.
    // Neither contributes, so neither is listed as a record behind a figure —
    // but the screen accounts for them rather than quietly dropping them.
    await expect(page.getByTestId('energy-row')).toHaveCount(3)
    await expect(page.getByTestId('energy-excluded')).toContainText('2')

    const table = page.getByTestId('energy-table')
    await expect(table).not.toContainText('Draft')
    await expect(table).not.toContainText('Rejected')

    // 12.000 + 18.000 = 30.000 raw, and 7.200 + 10.800 = 18.000 corrected.
    // Neither combined figure may appear: an application is not a load.
    await expect(table).not.toContainText('30.000 kW')
    await expect(page.locator('body')).toContainText(/never added together|separate figures/i)
  })

  test('an energy headline reaches the request behind it', async ({ page }) => {
    await openIlundoTower(page)

    await openDrill(page, 'tile-energy')
    await page.getByTestId('energy-row').first().getByTestId('drill-link').click()

    await expect(page.getByTestId('request-review')).toBeVisible()
    await expect(page.getByTestId('review-estimate')).toBeVisible()
  })

  test('a village with no capacity row still renders rather than erroring', async ({ page }) => {
    await signInAsOps(page)
    // Both seeded villages have capacity, so this exercises the guard on a
    // well-formed id that matches no capacity row.
    await page.goto('/ops/tower?village=30000000-0000-4000-8000-0000000000ff')
    await expect(page.getByTestId('tower')).toBeVisible()
    await expect(page.getByTestId('error-state')).toHaveCount(0)
  })
})
