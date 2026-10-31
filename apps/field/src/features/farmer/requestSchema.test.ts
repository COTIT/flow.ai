import { describe, expect, test } from 'vitest'

import { requestSchema, type RequestForm } from '@/features/farmer/requestSchema'

const VALID: RequestForm = {
  quantity: '1',
  hours_per_day: '6',
  days_per_week: '5',
  purpose: 'Kusaga mahindi ya kijiji',
}

const parse = (over: Partial<RequestForm> = {}) => requestSchema.safeParse({ ...VALID, ...over })

const issues = (result: ReturnType<typeof parse>) =>
  result.success ? [] : result.error.issues.map((i) => `${i.path.join('.')}:${i.message}`)

describe('a sane request', () => {
  test('passes', () => {
    expect(parse().success).toBe(true)
  })

  // `purpose` is nullable text on pue_request. The farmer surface should not
  // demand what the schema does not.
  test('passes without a purpose', () => {
    expect(issues(parse({ purpose: '' }))).toEqual([])
  })

  test('a purpose of spaces is stored as nothing', () => {
    const result = parse({ purpose: '   ' })
    expect(result.success).toBe(true)
    if (result.success) expect(result.data.purpose).toBe('')
  })

  // Fractional hours are normal — `numeric(4,2)`, and the seed uses 6.00.
  test('fractional hours and days are fine', () => {
    expect(parse({ hours_per_day: '5.5', days_per_week: '6.5' }).success).toBe(true)
  })

  test('the boundaries the column allows are allowed', () => {
    expect(parse({ hours_per_day: '24', days_per_week: '7' }).success).toBe(true)
  })
})

/**
 * QA #9. Every row of that finding's table: 99 hours rendered a confident
 * 1,485 kWh/day, 0 hours rendered 0 kWh with submit still enabled, and
 * quantity 0 rendered a 0 kW peak that the database then refused.
 *
 * `hours_per_day between 0 and 24` and `quantity > 0` are the columns' own
 * checks. Bounding an input to the range its column accepts is not a copy of a
 * business rule — it is the difference between an inline message and a round
 * trip that comes back as a constraint name.
 */
describe('the numbers have to be possible', () => {
  test('a day has 24 hours', () => {
    expect(issues(parse({ hours_per_day: '99' }))).toContain(
      'hours_per_day:equipment.hoursRange',
    )
    expect(issues(parse({ hours_per_day: '24.5' }))).toContain(
      'hours_per_day:equipment.hoursRange',
    )
  })

  test('a week has 7 days', () => {
    expect(issues(parse({ days_per_week: '8' }))).toContain('days_per_week:equipment.daysRange')
  })

  test('quantity is a whole number of machines', () => {
    expect(issues(parse({ quantity: '1.5' }))).toContain('quantity:equipment.wholeNumber')
  })

  // `quantity integer not null check (quantity > 0)`.
  test('zero machines is not a request', () => {
    expect(issues(parse({ quantity: '0' }))).toContain('quantity:equipment.moreThanZero')
  })

  /**
   * The column permits `0`, so this is the form being STRICTER than the
   * database rather than duplicating it. A request to run a mill for zero
   * hours asks for nothing, produces a 0 kWh estimate, and would sit in the
   * ops pipeline as a decision nobody can make. #9 asks for exactly this.
   */
  test('zero hours and zero days are refused even though the column allows them', () => {
    expect(issues(parse({ hours_per_day: '0' }))).toContain(
      'hours_per_day:equipment.moreThanZero',
    )
    expect(issues(parse({ days_per_week: '0' }))).toContain(
      'days_per_week:equipment.moreThanZero',
    )
  })

  // A negative is below zero, so "must be more than zero" is what is actually
  // wrong with it. "Between 0 and 24" would be true and unhelpful.
  test('negatives are refused, and told they are below zero', () => {
    expect(issues(parse({ hours_per_day: '-1' }))).toContain(
      'hours_per_day:equipment.moreThanZero',
    )
    expect(issues(parse({ days_per_week: '-1' }))).toContain(
      'days_per_week:equipment.moreThanZero',
    )
    expect(issues(parse({ quantity: '-2' }))).toContain('quantity:equipment.moreThanZero')
  })

  /**
   * `hours_per_day numeric(4,2)` and `days_per_week numeric(3,1)` round past
   * their scale without an error: 2.25 days is stored as 2.3, the farmer is
   * never told, and the retry reconciliation reports a false "earlier version
   * saved". So the form refuses what the column would silently change.
   */
  test('hours keep at most two decimal places', () => {
    expect(issues(parse({ hours_per_day: '2.25' }))).toEqual([])
    expect(issues(parse({ hours_per_day: '2.255' }))).toContain(
      'hours_per_day:equipment.hoursDecimals',
    )
  })

  test('days keep at most one decimal place', () => {
    expect(issues(parse({ days_per_week: '2.5' }))).toEqual([])
    expect(issues(parse({ days_per_week: '2.25' }))).toContain(
      'days_per_week:equipment.daysDecimals',
    )
  })

  test.each(['quantity', 'hours_per_day', 'days_per_week'] as const)('%s rejects text', (field) => {
    expect(issues(parse({ [field]: 'abc' } as Partial<RequestForm>))).toContain(
      `${field}:equipment.notANumber`,
    )
  })

  // Unlike the register form's optional figures, these three are the estimate's
  // inputs. A blank one is not "no answer", it is an estimate that cannot be
  // computed — `Number('')` is 0 and `energy_estimate` requires all three.
  test.each(['quantity', 'hours_per_day', 'days_per_week'] as const)(
    '%s cannot be left blank',
    (field) => {
      expect(issues(parse({ [field]: '' } as Partial<RequestForm>))).toContain(
        `${field}:equipment.required`,
      )
    },
  )
})
