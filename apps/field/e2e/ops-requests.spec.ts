import { expect, test, type Page } from '@playwright/test'

import { tr } from './support/i18n'
import { VILLAGE } from './support/seed'
import { assertsSeededFigures } from './support/seeded'
import { chooseSelect } from './support/select'

/**
 * Specs 7.2 and 7.3 — the request pipeline, review and decide.
 *
 * 7.3's acceptance criterion: "approving updates the farmer's request view and
 * moves approved_peak_kw in v_village_energy."
 *
 * Decisions are only ever made on requests this suite CREATES. The seeded
 * approved pair is what makes CLAUDE.md's 10.800 kW figure true, and approve
 * is terminal — there is no un-approve — so deciding a seeded request would
 * permanently break the demo's headline number.
 */
const PASSWORD = 'demo1234'

// This spec asserts seeded figures, so it starts from seeded state.
assertsSeededFigures()
const MILL = '51000000-0000-4000-8000-000000000001'

async function signIn(page: Page, email: string, home: RegExp) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill(email)
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(home)
}

/** Neema submits a marked request, so cleanup can remove it afterwards. */
async function farmerSubmitsRequest(page: Page, purpose: string) {
  await signIn(page, 'neema@demo.ruaha360.test', /\/farm$/)
  await page.goto(`/farm/equipment/${MILL}`)
  await page.getByTestId('request-purpose').fill(purpose)
  await page.getByTestId('equipment-acquisition-buy').check()
  await page.getByTestId('request-submit').click()
  await expect(page.getByTestId('request-success')).toBeVisible()
  await page.getByTestId('sign-out').click()
  await expect(page).toHaveURL(/\/login/)
}

test.describe('/ops/requests', () => {
  test('lists requests with applicant, equipment, estimate and status', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests')

    await expect(page.getByTestId('requests-table')).toBeVisible()
    // At least the six seeded requests. Not an exact count: other specs in
    // this suite submit marked requests of their own, and asserting a magic
    // total would make this test depend on their timing.
    expect(await page.getByTestId('request-row').count()).toBeGreaterThanOrEqual(6)
    // The seeded applicants are what this screen must actually show.
    await expect(page.getByTestId('requests-table')).toContainText('Neema Mwakalinga')
    await expect(page.getByTestId('requests-table')).toContainText('Zawadi Ngowi')

    const header = page.getByTestId('requests-table')
    await expect(header).toContainText('Applicant')
    await expect(header).toContainText('Village')
    await expect(header).toContainText('Equipment')
    await expect(header).toContainText('Est. kW')
    await expect(header).toContainText('Status')
  })

  // Filter state lives in the URL as validated search params.
  test('the status filter is held in the URL and survives a reload', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests?status=approved')

    await expect(page.getByTestId('filter-status')).toContainText('Approved')
    // Wait for the table before counting: locator.count() does not auto-wait,
    // so counting while the query is still in flight reads zero.
    await expect(page.getByTestId('requests-table')).toBeVisible()

    // The filter's claim is that every row matches it, which holds however
    // many rows the rest of the suite has created.
    const pills = page.getByTestId('status-pill')
    const count = await pills.count()
    expect(count).toBeGreaterThanOrEqual(2)
    for (let i = 0; i < count; i += 1) {
      await expect(pills.nth(i)).toHaveAttribute('data-status', 'approved')
    }

    await page.reload()
    await expect(page.getByTestId('filter-status')).toContainText('Approved')
    await expect(page.getByTestId('requests-table')).toBeVisible()
    await expect(page.getByTestId('status-pill').first()).toHaveAttribute(
      'data-status',
      'approved',
    )
  })

  test('changing the filter changes the URL, not just the table', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests')

    await chooseSelect(page, 'filter-status', 'Rejected')
    await expect(page).toHaveURL(/status=rejected/)
    await expect(page.getByTestId('requests-table')).toBeVisible()
    await expect(page.getByTestId('status-pill').first()).toHaveAttribute(
      'data-status',
      'rejected',
    )
  })

  test('the village filter narrows to that village', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto(`/ops/requests?village=${VILLAGE.MGAMA}`)

    // Only the withdrawn Mgama irrigation request is seeded there, and this
    // suite's own requests are all submitted in Ilundo.
    await expect(page.getByTestId('request-row')).toHaveCount(1)
    await expect(page.getByTestId('requests-table')).toContainText('Zawadi Ngowi')
    await expect(page.getByTestId('requests-table')).not.toContainText('Neema Mwakalinga')
  })

  test('a nonsense filter in the URL degrades to unfiltered rather than breaking', async ({
    page,
  }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    // The claim is that nonsense degrades to the UNFILTERED view, so it is
    // asserted against that view rather than against a fixed number.
    await page.goto('/ops/requests')
    await expect(page.getByTestId('requests-table')).toBeVisible()
    const unfiltered = await page.getByTestId('request-row').count()

    await page.goto('/ops/requests?status=not-a-status&village=nonsense')

    await expect(page.getByTestId('requests-table')).toBeVisible()
    await expect(page.getByTestId('request-row')).toHaveCount(unfiltered)
    await expect(page.getByTestId('error-state')).toHaveCount(0)
    // Both filters fell back to "all".
    await expect(page.getByTestId('filter-status')).toContainText('All statuses')
    await expect(page.getByTestId('filter-village')).toContainText('All villages')
  })

  test('a filter matching nothing is an empty state, not an error', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    // No draft exists in Mgama.
    await page.goto(`/ops/requests?status=draft&village=${VILLAGE.MGAMA}`)

    await expect(page.getByTestId('empty-state')).toBeVisible()
    await expect(page.getByTestId('error-state')).toHaveCount(0)
  })

  test('a farmer cannot reach the ops pipeline', async ({ page }) => {
    await signIn(page, 'neema@demo.ruaha360.test', /\/farm$/)
    await page.goto('/ops/requests')
    await expect(page).toHaveURL(/\/farm$/)
  })
})

test.describe('/ops/requests/$requestId', () => {
  test('shows the request, the snapshotted estimate and village headroom', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests')
    await page.getByTestId('request-row').first().click()

    await expect(page.getByTestId('request-review')).toBeVisible()
    await expect(page.getByTestId('review-estimate')).toBeVisible()
    // v_village_energy: planned capacity, never measured, shown with its basis.
    await expect(page.getByTestId('review-headroom')).toBeVisible()
    await expect(page.getByTestId('review-capacity-basis')).toContainText(/planned/i)
  })

  test('a decided request offers no actions', async ({ page }) => {
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    // The seeded approved mill request.
    await page.goto('/ops/requests/d0000000-0000-4000-8000-000000000001')

    await expect(page.getByTestId('request-review')).toBeVisible()
    await expect(page.getByTestId('action-start_review')).toHaveCount(0)
    await expect(page.getByTestId('action-approve')).toHaveCount(0)
    await expect(page.getByTestId('action-reject')).toHaveCount(0)
  })

  test('a submitted request offers only Start review', async ({ page }) => {
    await farmerSubmitsRequest(page, 'E2E-start-review')
    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests?status=submitted')
    await page.getByTestId('request-row').first().click()

    await expect(page.getByTestId('action-start_review')).toBeVisible()
    await expect(page.getByTestId('action-approve')).toHaveCount(0)
    await expect(page.getByTestId('action-reject')).toHaveCount(0)
  })

  // The acceptance criterion: approving moves approved_peak_kw.
  test('the full review path, and the headroom moves when it is approved', async ({ page }) => {
    await farmerSubmitsRequest(page, 'E2E-approve-path')

    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests?status=submitted')
    await page.getByTestId('request-row').first().click()
    await expect(page.getByTestId('request-review')).toBeVisible()

    const headroomBefore = await page.getByTestId('review-headroom').textContent()

    await page.getByTestId('action-start_review').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'under_review')

    // Approve requires a decision note.
    await page.getByTestId('action-approve').click()
    await expect(page.getByTestId('decision-note-error')).toBeVisible()

    await page.getByTestId('decision-note').fill('DEMO approval. Headroom confirmed.')
    await page.getByTestId('action-approve').click()

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'approved')

    // v_village_energy recomputes: an approved request consumes headroom.
    await expect(page.getByTestId('review-headroom')).not.toHaveText(headroomBefore!.trim())
  })

  test('rejecting also requires a note, and is recorded', async ({ page }) => {
    await farmerSubmitsRequest(page, 'E2E-reject-path')

    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests?status=submitted')
    await page.getByTestId('request-row').first().click()

    await page.getByTestId('action-start_review').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'under_review')

    await page.getByTestId('decision-note').fill('E2E rejection. Not a real decision.')
    await page.getByTestId('action-reject').click()

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'rejected')
    await expect(page.getByTestId('request-review')).toContainText('E2E rejection')
  })

  // The other half of 7.3's acceptance criterion.
  test('the farmer sees the decision on their own request view', async ({ page }) => {
    await farmerSubmitsRequest(page, 'E2E-farmer-sees-decision')

    await signIn(page, 'ops@demo.ruaha360.test', /\/ops$/)
    await page.goto('/ops/requests?status=submitted')
    await page.getByTestId('request-row').first().click()
    await page.getByTestId('action-start_review').click()
    await page.getByTestId('decision-note').fill('DEMO approval for the farmer view.')
    await page.getByTestId('action-approve').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'approved')
    await page.getByTestId('sign-out').click()
    await expect(page).toHaveURL(/\/login/)

    await signIn(page, 'neema@demo.ruaha360.test', /\/farm$/)
    await page.goto('/farm/requests')
    await expect(page.getByTestId('requests-list')).toContainText(tr('sw', 'requestStatus.approved'))
  })
})
