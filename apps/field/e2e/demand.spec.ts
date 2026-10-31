import { expect, test, type Page } from '@playwright/test'

import { assertsSeededFigures } from './support/seeded'
import { chooseSelect } from './support/select'

/**
 * Specs 7.6, 7.7 and 7.8 — demand, coverage, opportunity and supply.
 *
 * 7.7's acceptance: "the seeded maize demand shows Ilundo at 5,600 kg
 * available and 62.2% coverage. The seeded coffee demand shows zero matching
 * supply, and the screen says so rather than hiding the row."
 *
 * 7.8's acceptance: "attaching supply already committed elsewhere is refused
 * with a readable message."
 *
 * Records created here are marked so cleanup can remove them: buyer_demand
 * through quality_note, opportunity through note. The seeded figures are
 * specification — CLAUDE.md asserts 12,000 / 6,400 / 5,600 / 62.2% — so
 * nothing may be left behind.
 */
const PASSWORD = 'demo1234'

// This spec asserts seeded figures, so it starts from seeded state.
assertsSeededFigures()

// Seeded: Iringa Grain Traders wants 9,000 kg of maize in September.
const MAIZE_DEMAND = 'e1000000-0000-4000-8000-000000000001'
// Seeded: Highland Coffee Exporters, Jan–Feb 2027 — no coffee cycle matches.
const COFFEE_DEMAND = 'e1000000-0000-4000-8000-000000000002'
const ILUNDO = '30000000-0000-4000-8000-000000000001'
// The already-fully-committed Ilundo maize figure (4,100 kg, all promised).

async function signInAsOps(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/ops$/)
}

async function openDemandCreate(page: Page) {
  await page.getByTestId('demand-create-open').click()
  await expect(page.getByTestId('demand-create-panel')).toBeVisible()
}

test.describe('/ops/demand', () => {
  test('lists the seeded demands with buyer, crop and window', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/demand')

    await expect(page.getByTestId('demand-table')).toBeVisible()
    const table = page.getByTestId('demand-table')
    await expect(table).toContainText('Iringa Grain Traders')
    await expect(table).toContainText('Highland Coffee Exporters')
    await expect(table).toContainText('9,000.00 kg')
  })

  // Prices are indicative, never quotations.
  test('the indicative price is labelled indicative', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/demand')
    await expect(page.getByTestId('demand-table')).toContainText(/indicative/i)
  })

  test('creating a demand requires the fields the schema requires', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/demand')
    await openDemandCreate(page)

    await page.getByTestId('demand-create-submit').click()
    await expect(page.getByTestId('demand-buyer-error')).toBeVisible()
    await expect(page.getByTestId('demand-crop-error')).toBeVisible()
    await expect(page.getByTestId('demand-quantity-error')).toBeVisible()
  })

  test('a created demand appears in the list', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/demand')
    await openDemandCreate(page)

    await chooseSelect(page, 'demand-buyer', 'Iringa Grain Traders')
    await chooseSelect(page, 'demand-crop', 'Maize')
    await page.getByTestId('demand-quantity').fill('2500')
    await page.getByTestId('demand-window-start').fill('2026-09-01')
    await page.getByTestId('demand-window-end').fill('2026-09-30')
    await page.getByTestId('demand-quality-note').fill('E2E-created-demand')
    await page.getByTestId('demand-create-submit').click()

    await expect(page.getByTestId('demand-table')).toContainText('E2E-created-demand')
  })

  // demand_window_sane: window_end >= window_start. Still not pre-checked —
  // the constraint is called. What changed in M7 is what the user READS: a
  // constraint identifier is not copy in any language (QA #21, #4).
  test('a backwards window is refused with the database message', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/demand')
    await openDemandCreate(page)

    await chooseSelect(page, 'demand-buyer', 'Iringa Grain Traders')
    await chooseSelect(page, 'demand-crop', 'Maize')
    await page.getByTestId('demand-quantity').fill('100')
    await page.getByTestId('demand-window-start').fill('2026-09-30')
    await page.getByTestId('demand-window-end').fill('2026-09-01')
    await page.getByTestId('demand-quality-note').fill('E2E-backwards-window')
    await page.getByTestId('demand-create-submit').click()

    const error = page.getByTestId('demand-create-error')
    await expect(error).toContainText(/must end on or after it starts/i)
    await expect(error).not.toContainText(/demand_window_sane|buyer_demand/i)
  })

  test('a farmer cannot reach the order book', async ({ page }) => {
    await page.goto('/login')
    await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/farm$/)

    await page.goto('/ops/demand')
    await expect(page).toHaveURL(/\/farm$/)
  })
})

test.describe('/ops/demand/$demandId', () => {
  // The acceptance criterion, first half.
  test('the maize demand shows Ilundo at 5,600 kg available and 62.2% coverage', async ({
    page,
  }) => {
    await signInAsOps(page)
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)

    await expect(page.getByTestId('demand-detail')).toBeVisible()
    const row = page.getByTestId(`match-row-${ILUNDO}`)
    await expect(row).toContainText('5,600.00 kg')
    await expect(row).toContainText('62.2%')
  })

  test('the committed slice is visible, not folded into the total', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)

    // Scoped to Ilundo: Mgama also has September maize seeded (2,700 kg), so
    // this demand legitimately matches two villages and there is one coverage
    // block per village.
    const ilundo = page.getByTestId(`coverage-${ILUNDO}`)
    await expect(ilundo.getByTestId('coverage-committed')).toContainText('6,400.00 kg')
    await expect(ilundo.getByTestId('coverage-available')).toContainText('5,600.00 kg')
  })

  // The acceptance criterion, second half: an honest zero, stated.
  test('the coffee demand says it has no matching supply rather than hiding it', async ({
    page,
  }) => {
    await signInAsOps(page)
    await page.goto(`/ops/demand/${COFFEE_DEMAND}`)

    await expect(page.getByTestId('demand-detail')).toBeVisible()
    // The demand itself is still shown — it is not hidden for lack of supply.
    await expect(page.getByTestId('demand-detail')).toContainText('Highland Coffee Exporters')
    await expect(page.getByTestId('no-matching-supply')).toBeVisible()
    await expect(page.getByTestId('match-row-' + ILUNDO)).toHaveCount(0)
  })

  test('an existing opportunity is shown rather than offered again', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)

    // Ilundo already has the seeded demo opportunity on this demand.
    const row = page.getByTestId(`match-row-${ILUNDO}`)
    await expect(row.getByTestId('drill-link')).toBeVisible()
    await expect(row.getByTestId('create-opportunity')).toHaveCount(0)
  })

  test('states plainly that an opportunity is not a sale', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await expect(page.getByTestId('demand-detail')).toContainText(/not a sale/i)
  })
})

test.describe('/ops/opportunities/$opportunityId', () => {
  test('shows the supply lines, each drilling to its record', async ({ page }) => {
    await signInAsOps(page)
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await page.getByTestId(`match-row-${ILUNDO}`).getByTestId('drill-link').click()

    await expect(page.getByTestId('opportunity-detail')).toBeVisible()
    // Seeded: 4,100 + 2,300 = 6,400 kg across two supply lines.
    await expect(page.getByTestId('supply-row')).toHaveCount(2)
    await expect(page.getByTestId('opportunity-detail')).toContainText('4,100.00 kg')
    await expect(page.getByTestId('opportunity-detail')).toContainText('2,300.00 kg')
    await expect(page.getByTestId('offered-total')).toContainText('6,400.00 kg')
  })

  test('states plainly that it is not a sale, delivery or payment', async ({ page }) => {
    await signInAsOps(page)
    await page.goto('/ops/opportunities/e2000000-0000-4000-8000-000000000001')
    await expect(page.getByTestId('opportunity-detail')).toContainText(/not a sale/i)
  })

  // 7.8's acceptance criterion. opportunity_supply_guard's message is shown as
  // written — not pre-checked in the client, not swallowed.
  //
  // The claim has to come from a DIFFERENT opportunity. Attaching the same
  // harvest twice to the same opportunity hits
  // unique (opportunity_id, harvest_report_id) first, which is a different
  // refusal and not the one this criterion is about.
  test('supply already committed elsewhere is refused with the readable message', async ({
    page,
  }) => {
    await signInAsOps(page)

    // A second maize demand over the same September window, so Ilundo's
    // already-committed harvest is reachable from a fresh opportunity.
    await page.goto('/ops/demand')
    await openDemandCreate(page)
    await chooseSelect(page, 'demand-buyer', 'Iringa Grain Traders')
    await chooseSelect(page, 'demand-crop', 'Maize')
    await page.getByTestId('demand-quantity').fill('4000')
    await page.getByTestId('demand-window-start').fill('2026-09-01')
    await page.getByTestId('demand-window-end').fill('2026-09-30')
    await page.getByTestId('demand-quality-note').fill('E2E-double-commit')
    await page.getByTestId('demand-create-submit').click()
    await expect(page.getByTestId('demand-table')).toContainText('E2E-double-commit')

    await page.getByTestId('demand-row').filter({ hasText: 'E2E-double-commit' }).click()
    await expect(page.getByTestId('demand-detail')).toBeVisible()

    await page
      .getByTestId(`match-row-${ILUNDO}`)
      .getByTestId('create-opportunity')
      .click()
    await page.getByTestId(`match-row-${ILUNDO}`).getByTestId('drill-link').click()
    await expect(page.getByTestId('opportunity-detail')).toBeVisible()

    // c0000000-…02 is 4,100 kg and entirely promised to the seeded
    // opportunity, so any claim on it must be refused.
    await chooseSelect(page, 'attach-harvest', '4,100.00 kg')
    await page.getByTestId('attach-kg').fill('100')
    await page.getByTestId('attach-submit').click()

    // business-rules §9's other half, and the one M7 must not break: the
    // guard's own message is written to be read and is shown AS WRITTEN. It
    // names the numbers, which is the only part worth reading.
    await expect(page.getByTestId('attach-error')).toContainText(/over-commitment/i)
    await expect(page.getByTestId('attach-error')).toContainText('4100.00 kg available')
    await expect(page.getByTestId('attach-error')).toContainText('4100.00 kg already committed')
    await expect(page.getByTestId('attach-error')).toContainText('100.00 kg requested')
  })

  test('the offered total is re-summed by the database, never set by the client', async ({
    page,
  }) => {
    await signInAsOps(page)
    await page.goto('/ops/opportunities/e2000000-0000-4000-8000-000000000001')

    // offered_quantity_kg equals the sum of its supply lines because
    // opportunity_resum maintains it. The tile and the drill-down cannot drift.
    await expect(page.getByTestId('offered-total')).toContainText('6,400.00 kg')
    await expect(page.getByTestId('offered-total-note')).toBeVisible()
  })
})
