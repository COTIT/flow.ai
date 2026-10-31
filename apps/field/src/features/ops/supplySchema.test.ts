import { describe, expect, test } from 'vitest'

import { supplySchema, type SupplyForm } from '@/features/ops/supplySchema'

const VALID: SupplyForm = { harvest_report_id: 'h1', contributed_kg: '1600' }

const parse = (over: Partial<SupplyForm> = {}) => supplySchema.safeParse({ ...VALID, ...over })
const issues = (result: ReturnType<typeof parse>) =>
  result.success ? [] : result.error.issues.map((i) => `${i.path.join('.')}:${i.message}`)

/**
 * The attach-supply form — QA #21's tail:
 * "`opportunity_supply.contributed_kg` is `check (contributed_kg > 0)`, so 0 or
 * a negative contribution on the attach-supply form will surface the same way"
 * — as a raw constraint name.
 *
 * **What this schema must NOT do.** It checks the column's own shape and
 * nothing else. `opportunity_supply_guard` owns over-commitment, its message
 * names the real numbers, and business-rules §8 says to show it as written. A
 * client-side check of `available_kg` would be exactly the copy CLAUDE.md
 * forbids — and would be wrong the moment another opportunity committed the
 * same harvest.
 */
describe('the attach-supply form', () => {
  test('a real contribution passes', () => {
    expect(parse().success).toBe(true)
  })

  test('a harvest figure has to be chosen', () => {
    expect(issues(parse({ harvest_report_id: '' }))).toContain(
      'harvest_report_id:opportunity.chooseHarvestRequired',
    )
  })

  test('a blank contribution is not a zero', () => {
    expect(issues(parse({ contributed_kg: '' }))).toContain(
      'contributed_kg:opportunity.kgRequired',
    )
  })

  // `check (contributed_kg > 0)`: zero is not a contribution.
  test('zero and negative are refused', () => {
    expect(issues(parse({ contributed_kg: '0' }))).toContain(
      'contributed_kg:opportunity.kgMoreThanZero',
    )
    expect(issues(parse({ contributed_kg: '-5' }))).toContain(
      'contributed_kg:opportunity.kgMoreThanZero',
    )
  })

  test('text is refused', () => {
    expect(issues(parse({ contributed_kg: 'abc' }))).toContain(
      'contributed_kg:opportunity.kgNotANumber',
    )
  })

  // numeric(12,2).
  test('a figure past the column scale is refused', () => {
    expect(issues(parse({ contributed_kg: '999999999999' }))).toContain(
      'contributed_kg:opportunity.kgTooLarge',
    )
  })

  test('fractional kilograms are fine — the column holds two decimals', () => {
    expect(parse({ contributed_kg: '1600.55' }).success).toBe(true)
  })

  // numeric(12,2) rounds a third decimal silently rather than refusing it.
  test('more than two decimal places is refused rather than silently rounded', () => {
    expect(parse({ contributed_kg: '1.5' }).success).toBe(true)
    expect(issues(parse({ contributed_kg: '1.555' }))).toContain(
      'contributed_kg:opportunity.kgDecimals',
    )
  })

  /**
   * The line that matters most. A quantity within the column's range but past
   * what is available must still PASS here, so the guard is what refuses it
   * and its message is what the user reads.
   */
  test('a quantity beyond what is available still passes — the guard refuses that', () => {
    expect(parse({ contributed_kg: '99999' }).success).toBe(true)
  })
})
