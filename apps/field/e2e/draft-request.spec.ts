import { expect, test, type Page } from '@playwright/test'

import { sql } from './support/db'
import { markedName } from './support/marker'

/**
 * The farmer's half of the request status machine — business-rules §2's role
 * matrix, farmer column: a draft may be SUBMITTED or WITHDRAWN by the person
 * who owns it.
 *
 * **Why this file needs its own fixture.** A farmer has no route to a draft:
 * the equipment form submits straight to `submitted`, so the only draft in the
 * system is the seeded cold room. The status machine is one-way, and
 * `cleanup.sql` removes marked ROWS — it cannot undo a column change on a row
 * the seed created. Submitting the seeded draft would consume it permanently
 * and the second run of this file would fail.
 *
 * So each test inserts its own draft, marked through `purpose`, which is
 * exactly what `cleanup.sql` already matches on. This was a recorded coverage
 * gap: the transitions had unit tests and no end-to-end proof.
 */
const PASSWORD = 'demo1234'
const NEEMA = '60000000-0000-4000-8000-000000000001'
const ILUNDO = '30000000-0000-4000-8000-000000000001'
/** Seeded cold room — the equipment the seeded draft also uses. */
const COLD_ROOM = '51000000-0000-4000-8000-000000000005'

/**
 * A draft belonging to Neema, marked so cleanup can find it.
 *
 * `pue_request_guard` permits INSERT as draft or submitted, so this is a state
 * the schema allows rather than one forced past it.
 */
function createDraft(purpose: string): string {
  const result = sql(
    `insert into pue_request
       (village_id, person_id, equipment_id, quantity, hours_per_day, days_per_week,
        purpose, status, source, confidence)
     values
       ('${ILUNDO}', '${NEEMA}', '${COLD_ROOM}', 1, 8.00, 5.0,
        '${purpose}', 'draft', 'farmer_reported', 'low')
     returning id`,
  )
  if (!result.ran) throw new Error(`could not create a draft: ${result.reason}`)
  return result.out
}

function statusOf(id: string): string {
  return sql(`select status from pue_request where id = '${id}'`).out
}

async function signInAsNeema(page: Page) {
  await page.goto('/login')
  await page.getByTestId('login-email').fill('neema@demo.ruaha360.test')
  await page.getByTestId('login-password').fill(PASSWORD)
  await page.getByTestId('login-submit').click()
  await expect(page).toHaveURL(/\/farm$/)
}

test.describe('a farmer acting on their own draft', () => {
  test('submits it, and it reaches the ops pipeline', async ({ page }) => {
    const purpose = `${markedName()} submit`
    const id = createDraft(purpose)

    await signInAsNeema(page)
    await page.goto(`/farm/requests/${id}`)
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'draft')

    await page.getByTestId('request-action-submit').click()

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'submitted')
    expect(statusOf(id)).toBe('submitted')
  })

  test('or withdraws it, and it is gone from the queue without being deleted', async ({ page }) => {
    const purpose = `${markedName()} withdraw`
    const id = createDraft(purpose)

    await signInAsNeema(page)
    await page.goto(`/farm/requests/${id}`)

    await page.getByTestId('request-action-withdraw').click()

    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'withdrawn')
    // The record survives: a withdrawn request is still a record of an ask.
    expect(statusOf(id)).toBe('withdrawn')
  })

  // Withdrawn is terminal in the machine, so nothing may offer a way out of it.
  test('a withdrawn request offers nothing further', async ({ page }) => {
    const id = createDraft(`${markedName()} terminal`)

    await signInAsNeema(page)
    await page.goto(`/farm/requests/${id}`)
    await page.getByTestId('request-action-withdraw').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'withdrawn')

    await expect(page.getByTestId('request-action-submit')).toHaveCount(0)
    await expect(page.getByTestId('request-action-withdraw')).toHaveCount(0)
  })

  // Once submitted, the request belongs to the reviewer. Starting a review is
  // ops's move, and the farmer's remaining action is to withdraw.
  test('a submitted request can still be withdrawn, but not re-submitted', async ({ page }) => {
    const id = createDraft(`${markedName()} submitted`)

    await signInAsNeema(page)
    await page.goto(`/farm/requests/${id}`)
    await page.getByTestId('request-action-submit').click()
    await expect(page.getByTestId('status-pill')).toHaveAttribute('data-status', 'submitted')

    await expect(page.getByTestId('request-action-submit')).toHaveCount(0)
    await expect(page.getByTestId('request-action-withdraw')).toBeVisible()
  })

  // The draft belongs to its owner. Another farmer must not see it at all —
  // and that is RLS, not the route guard.
  test('another farmer cannot reach it', async ({ page }) => {
    const id = createDraft(`${markedName()} private`)

    await page.goto('/login')
    await page.getByTestId('login-email').fill('joseph@demo.ruaha360.test')
    await page.getByTestId('login-password').fill(PASSWORD)
    await page.getByTestId('login-submit').click()
    await expect(page).toHaveURL(/\/farm$/)

    await page.goto(`/farm/requests/${id}`)

    // Zero rows is the answer RLS gives, and the screen says "not found".
    await expect(page.getByTestId('empty-state')).toBeVisible()
    expect(statusOf(id)).toBe('draft')
  })
})
