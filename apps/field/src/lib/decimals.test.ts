import { describe, expect, test } from 'vitest'

import { hasAtMostDecimals } from '@/lib/decimals'

/**
 * Postgres `numeric(p,s)` rounds a value past its scale silently: 2.25 sent to
 * `numeric(3,1)` is stored as 2.3, nobody is told, and the retry
 * reconciliation then sees a stored value that differs from the submitted one.
 */
describe('hasAtMostDecimals', () => {
  test.each([
    [2.25, 2, true],
    [2.255, 2, false],
    [2.5, 1, true],
    [2.25, 1, false],
    [1.5, 2, true],
    [1.555, 2, false],
    [10, 0, true],
    [10.5, 0, false],
    [0.1, 1, true],
    [1.13, 2, true],
    [9_999_999_999.99, 2, true],
  ])('%s with at most %s places is %s', (n, places, expected) => {
    expect(hasAtMostDecimals(n, places)).toBe(expected)
  })

  // Trailing zeros are not extra precision: `Number('2.250')` is 2.25.
  test('a number typed with trailing zeros is judged by its value', () => {
    expect(hasAtMostDecimals(Number('2.250'), 2)).toBe(true)
  })

  test('a value that is not a finite number never qualifies', () => {
    expect(hasAtMostDecimals(Number.NaN, 2)).toBe(false)
    expect(hasAtMostDecimals(Number.POSITIVE_INFINITY, 2)).toBe(false)
  })
})
