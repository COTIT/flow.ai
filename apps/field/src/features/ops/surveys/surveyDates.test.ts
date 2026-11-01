import { describe, expect, test } from 'vitest'

import {
  addDays,
  closesOnToTimestamp,
  darToday,
  defaultRedemptionRange,
  timestampToDarDate,
  validateRedemptionSearch,
} from '@/features/ops/surveys/surveyDates'

/** Africa/Dar_es_Salaam is UTC+3 all year: no daylight saving to model. */
describe('darToday', () => {
  test('is the Tanzanian calendar day, not the UTC one', () => {
    // 22:30 UTC on the 28th is 01:30 on the 29th in Dar es Salaam.
    expect(darToday(new Date('2026-09-28T22:30:00Z'))).toBe('2026-09-29')
    expect(darToday(new Date('2026-09-28T20:59:00Z'))).toBe('2026-09-28')
  })
})

describe('addDays', () => {
  test('moves a plain date without a timezone anywhere near it', () => {
    expect(addDays('2026-09-29', -7)).toBe('2026-09-22')
    expect(addDays('2026-10-02', -7)).toBe('2026-09-25')
    expect(addDays('2026-01-03', -7)).toBe('2025-12-27')
    expect(addDays('2026-02-28', 1)).toBe('2026-03-01')
  })
})

describe('defaultRedemptionRange', () => {
  test('is today in Dar es Salaam back seven days', () => {
    expect(defaultRedemptionRange(new Date('2026-09-28T22:30:00Z'))).toEqual({
      from: '2026-09-22',
      to: '2026-09-29',
    })
  })
})

describe('validateRedemptionSearch', () => {
  test('keeps well-formed dates', () => {
    expect(validateRedemptionSearch({ from: '2026-09-01', to: '2026-09-29' })).toEqual({
      from: '2026-09-01',
      to: '2026-09-29',
    })
  })

  // A stale bookmark degrades to the default range rather than reaching the
  // RPC as an invalid date.
  test('drops anything that is not a YYYY-MM-DD date', () => {
    expect(validateRedemptionSearch({ from: 'yesterday', to: '2026-13-01' })).toEqual({})
    expect(validateRedemptionSearch({ from: 20260901, to: '2026-9-1' })).toEqual({})
    expect(validateRedemptionSearch({ from: '2026-02-31' })).toEqual({})
  })

  test('ignores unknown params', () => {
    expect(validateRedemptionSearch({ other: 'x', to: '2026-09-29' })).toEqual({ to: '2026-09-29' })
  })
})

describe('the closing date', () => {
  // "Closes on 30 Sep" means farmers can answer all of 30 September, in
  // Tanzanian time.
  test('a chosen day closes at the end of that day in Dar es Salaam', () => {
    expect(closesOnToTimestamp('2026-09-30')).toBe('2026-09-30T23:59:59+03:00')
  })

  test('a blank day is no closing date', () => {
    expect(closesOnToTimestamp('')).toBeNull()
  })

  test('a stored timestamp reads back as its Tanzanian day', () => {
    expect(timestampToDarDate('2026-09-30T20:59:59+00:00')).toBe('2026-09-30')
    expect(timestampToDarDate('2026-09-30T21:30:00+00:00')).toBe('2026-10-01')
    expect(timestampToDarDate(null)).toBe('')
    expect(timestampToDarDate('not a date')).toBe('')
  })
})
