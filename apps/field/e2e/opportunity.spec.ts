import { expect, test, type Page } from '@playwright/test'

import { trRe } from './support/i18n'
import { assertsSeededFigures } from './support/seeded'
import { chooseSelect } from './support/select'

/**
 * Spec 7.8's other half — the opportunity's status machine, and what moving it
 * does to everyone else's numbers.
 *
 * business-rules §7: `committed_kg` sums `opportunity_supply` on opportunities
 * in `proposed`, `shared` or `accepted`. **Declined and lapsed release their
 * supply.** There is no detach and no delete — `opportunity_supply` has
 * neither a DELETE policy nor a `deleted_at` — so leaving that set IS how a
 * commitment is unwound, and this file is the proof that it works end to end.
 *
 * Everything here happens on records the suite creates and marks. The SEEDED
 * opportunity is never touched: declining is one-way in the UI, by design, and
 * `cleanup.sql` removes marked ROWS and so could never undo a status written
 * onto a seeded one.
 */
const PASSWORD = 'demo1234'

assertsSeededFigures()

// Seeded: Iringa Grain Traders wants 9,000 kg of maize in September. Its
// Ilundo coverage is CLAUDE.md's headline figure — 5,600 kg, 62.2%.
const MAIZE_DEMAND = 'e1000000-0000-4000-8000-000000000001'
const ILUNDO = '30000000-0000-4000-8000-000000000001'
// Ilundo's one wholly uncommitted September maize figure: 5,600 kg, nothing
// promised. The other two (4,100 + 2,300) are the seeded opportunity's. It is
// Joseph's harvest — plot a0000000-…03, farm 90000000-…02.
/** Enough to move the headline unmistakably, and well inside 5,600. */
const CLAIM_KG = '1600'

async function signInAsOps(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('ops@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/ops$/)
}

/**
 * A marked demand over the seeded September maize window, with its own
 * opportunity on Ilundo. Returns the opportunity's URL.
 *
 * A fresh demand rather than the seeded one because the seeded demand already
 * carries the seeded opportunity, and `v_demand_match` left-joins one
 * opportunity per (demand, village).
 */
async function createMarkedOpportunity(page: Page, note: string): Promise<string> {
  await page.goto('/ops/demand')
  await page.getByTestId('demand-create-open').click()
  await expect(page.getByTestId('demand-create-panel')).toBeVisible()
  await chooseSelect(page, 'demand-buyer', 'Iringa Grain Traders')
  await chooseSelect(page, 'demand-crop', 'Maize')
  await page.getByTestId('demand-quantity').fill('4000')
  await page.getByTestId('demand-window-start').fill('2026-09-01')
  await page.getByTestId('demand-window-end').fill('2026-09-30')
  await page.getByTestId('demand-quality-note').fill(note)
  await page.getByTestId('demand-create-submit').click()
  await expect(page.getByTestId('demand-table')).toContainText(note)

  await page.getByTestId('demand-row').filter({ hasText: note }).click()
  await expect(page.getByTestId('demand-detail')).toBeVisible()

  await page.getByTestId(`match-row-${ILUNDO}`).getByTestId('create-opportunity').click()
  await page.getByTestId(`match-row-${ILUNDO}`).getByTestId('drill-link').click()
  await expect(page.getByTestId('opportunity-detail')).toBeVisible()

  return page.url()
}

test.describe('the opportunity status machine', () => {
  test('a new opportunity is proposed, and can be shared but not accepted', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-machine-proposed')

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'proposed')
    await expect(page.getByTestId('action-share')).toBeVisible()
    await expect(page.getByTestId('action-decline')).toBeVisible()
    await expect(page.getByTestId('action-lapse')).toBeVisible()
    // `accepted` describes the buyer's answer, and nobody has shown them anything.
    await expect(page.getByTestId('action-accept')).toHaveCount(0)
  })

  test('sharing moves it forward and opens the accept control', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-machine-shared')

    await page.getByTestId('action-share').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'shared')
    await expect(page.getByTestId('action-accept')).toBeVisible()
    await expect(page.getByTestId('action-share')).toHaveCount(0)
  })

  // §8: accepted means both sides agreed to keep talking. It is still not a
  // sale, and the screen must keep saying so at exactly this point.
  test('accepted is reachable, and still says nothing has moved', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-machine-accepted')

    await page.getByTestId('action-share').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'shared')
    await page.getByTestId('action-accept').click()

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'accepted')
    await expect(page.getByTestId('opportunity-detail')).toContainText(/not a sale/i)
    // A buyer can still walk away; a window still passes.
    await expect(page.getByTestId('action-decline')).toBeVisible()
    await expect(page.getByTestId('action-lapse')).toBeVisible()
  })

  test('declining is confirmed before it happens, and can be called off', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-machine-cancel')

    await page.getByTestId('action-decline').click()
    await expect(page.getByTestId('release-confirm')).toBeVisible()
    await page.getByTestId('release-confirm-no').click()

    await expect(page.getByTestId('release-confirm')).toHaveCount(0)
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'proposed')
  })

  test('a declined opportunity is terminal and offers nothing', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-machine-terminal')

    await page.getByTestId('action-decline').click()
    await page.getByTestId('release-confirm-yes').click()

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'declined')
    await expect(page.getByTestId('opportunity-actions')).toHaveCount(0)
    await expect(page.getByTestId('released-note')).toBeVisible()
    // Nothing more can be committed to something that counts toward nothing.
    await expect(page.getByTestId('attach-closed')).toBeVisible()
    await expect(page.getByTestId('attach-submit')).toHaveCount(0)
  })
})

/**
 * M5's acceptance criterion, and the reason the status machine matters at all.
 */
test.describe('declining releases the committed supply', () => {
  test('the seeded demand coverage moves when supply is committed, and returns when it is declined', async ({
    page,
  }) => {
    await signInAsOps(page)

    // The baseline, which is CLAUDE.md's headline figure.
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('5,600.00 kg')
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('62.2%')

    const opportunityUrl = await createMarkedOpportunity(page, 'E2E-release-cycle')

    // 1,600 kg out of the 5,600 nobody has promised.
    await chooseSelect(page, 'attach-harvest', '5,600.00 kg')
    // The picker names the farmer before Attach, and it is the farmer the
    // supply line lists after it.
    const picked = (await page.getByTestId('harvest-chosen-farmer').innerText()).split(' · ')[0].trim()
    expect(picked).not.toBe('—')
    await page.getByTestId('attach-kg').fill(CLAIM_KG)
    await page.getByTestId('attach-submit').click()
    await expect(page.getByTestId('supply-row')).toHaveCount(1)
    await expect(page.getByTestId('supply-row')).toContainText(picked)
    // And it says who attached it: the signed-in ops account, not a dash.
    await expect(page.getByTestId('supply-attached-by')).not.toHaveText(/^—/)
    await expect(page.getByTestId('offered-total')).toContainText('1,600.00 kg')

    // The seeded demand now sees less of Ilundo: 5,600 − 1,600 = 4,000, and
    // 4,000 / 9,000 = 44.4%. Nothing about the seeded demand changed; the
    // supply underneath it did.
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('4,000.00 kg')
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('44.4%')
    await expect(page.getByTestId(`coverage-${ILUNDO}`).getByTestId('coverage-committed')).toContainText(
      '8,000.00 kg',
    )

    // Decline it. No detach, no delete: the status IS the release.
    await page.goto(opportunityUrl)
    await page.getByTestId('action-decline').click()
    await page.getByTestId('release-confirm-yes').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'declined')

    // …and the 1,600 kg is back.
    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('5,600.00 kg')
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('62.2%')
    await expect(page.getByTestId(`coverage-${ILUNDO}`).getByTestId('coverage-committed')).toContainText(
      '6,400.00 kg',
    )
  })

  test('lapsing releases it the same way', async ({ page }) => {
    await signInAsOps(page)
    const opportunityUrl = await createMarkedOpportunity(page, 'E2E-release-lapse')

    await chooseSelect(page, 'attach-harvest', '5,600.00 kg')
    await page.getByTestId('attach-kg').fill(CLAIM_KG)
    await page.getByTestId('attach-submit').click()
    await expect(page.getByTestId('offered-total')).toContainText('1,600.00 kg')

    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('4,000.00 kg')

    await page.goto(opportunityUrl)
    await page.getByTestId('action-lapse').click()
    await page.getByTestId('release-confirm-yes').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'lapsed')

    await page.goto(`/ops/demand/${MAIZE_DEMAND}`)
    await expect(page.getByTestId(`match-row-${ILUNDO}`)).toContainText('5,600.00 kg')
  })

  // The supply line is the traceability record. Releasing must not erase it,
  // or "every headline traces back to records" stops being true for anything
  // that was ever offered and withdrawn.
  test('the released supply line stays on the record', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-release-kept')

    await chooseSelect(page, 'attach-harvest', '5,600.00 kg')
    await page.getByTestId('attach-kg').fill(CLAIM_KG)
    await page.getByTestId('attach-submit').click()
    await expect(page.getByTestId('supply-row')).toHaveCount(1)

    await page.getByTestId('action-decline').click()
    await page.getByTestId('release-confirm-yes').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'declined')

    await expect(page.getByTestId('supply-row')).toHaveCount(1)
    await expect(page.getByTestId('offered-total')).toContainText('1,600.00 kg')
  })

  // The farmer whose harvest it was sees the outcome, on her own screen, with
  // no buyer details she is not allowed to see.
  test('the contributing farmer sees his released opportunity as declined', async ({ page }) => {
    await signInAsOps(page)
    await createMarkedOpportunity(page, 'E2E-release-farmer')

    await chooseSelect(page, 'attach-harvest', '5,600.00 kg')
    await page.getByTestId('attach-kg').fill(CLAIM_KG)
    await page.getByTestId('attach-submit').click()
    await expect(page.getByTestId('supply-row')).toHaveCount(1)

    await page.getByTestId('action-decline').click()
    await page.getByTestId('release-confirm-yes').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'declined')

    // Switching accounts now means signing out first: a signed-in visitor is
    // sent away from /login rather than shown a form beside their own "Sign
    // out" button (QA #26).
    await page.getByTestId('sign-out').click()
    await expect(page).toHaveURL(/\/login$/)

    // c0000000-…04 hangs off plot a0000000-…03, which is Joseph's farm
    // (90000000-…02), NOT Neema's. `opportunity_read_contributor` is what puts
    // this row in front of him and keeps it away from every other farmer.
    await page.getByTestId('login-email').fill('joseph@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/farm$/)

    await page.goto('/farm/opportunities')
    await expect(page.getByTestId('farmer-opportunities')).toBeVisible()

    // Scoped to a row, and to the first of them: the earlier tests in this
    // file each leave Joseph a released opportunity of his own, so "a declined
    // pill exists" would match several. What is being asserted is that the
    // release reaches HIS screen with his own kilograms on it.
    const row = page
      .getByTestId('farmer-opportunity')
      .filter({ hasText: trRe('sw', 'opportunityStatus.declined', undefined, { flags: 'i' }) })
      .first()
    await expect(row).toBeVisible()
    await expect(row.getByTestId('my-contribution')).toContainText('1,600.00 kg')
    // demand_read is staff-only. No buyer name reaches a farmer, ever.
    await expect(row).toContainText(
      trRe('sw', 'farmerOpportunities.buyerWithOps', undefined, { flags: 'i' }),
    )
  })
})
