import { describe, expect, test } from 'vitest'

import { amountToMinor, countAndAmount, minorToAmount } from '@/features/ops/surveys/figures'

describe('typed amount to integer minor units', () => {
  test('whole amounts, with or without grouping', () => {
    expect(amountToMinor('5000', 2)).toBe(500000)
    expect(amountToMinor('5,000', 2)).toBe(500000)
    expect(amountToMinor(' 5 000 ', 2)).toBe(500000)
    expect(amountToMinor('5000', 0)).toBe(5000)
  })

  // 0.1 + 0.2 territory: string maths, never a float multiply.
  test('decimals are exact', () => {
    expect(amountToMinor('10.1', 2)).toBe(1010)
    expect(amountToMinor('0.29', 2)).toBe(29)
    expect(amountToMinor('19.99', 2)).toBe(1999)
    expect(amountToMinor('5000.', 2)).toBe(500000)
  })

  test('what is not an amount in this currency is null, for the API to answer', () => {
    expect(amountToMinor('', 2)).toBeNull()
    expect(amountToMinor('abc', 2)).toBeNull()
    expect(amountToMinor('-5', 2)).toBeNull()
    expect(amountToMinor('1.234', 2)).toBeNull()
    expect(amountToMinor('1.5', 0)).toBeNull()
    expect(amountToMinor('1e3', 2)).toBeNull()
  })
})

describe('integer minor units back to form text', () => {
  test('integer maths, padded', () => {
    expect(minorToAmount(500000, 2)).toBe('5000.00')
    expect(minorToAmount(5, 2)).toBe('0.05')
    expect(minorToAmount(5000, 0)).toBe('5000')
  })

  test('round-trips', () => {
    expect(amountToMinor(minorToAmount(1999, 2), 2)).toBe(1999)
  })
})

describe('a count and its cash', () => {
  test("formats the API's figures without deriving any", () => {
    expect(countAndAmount({ count: 3, minor: 1500000 }, 'TZS', 2, 'en')).toMatch(/^3 · TZS\s15,000\.00$/)
  })

  test('an absent figure is a dash, not a zero', () => {
    expect(countAndAmount(undefined, 'TZS', 2, 'en')).toBe('—')
  })
})

test('an amount too large to be an exact integer is refused, not rounded', () => {
  expect(amountToMinor('99999999999999999', 2)).toBeNull()
  expect(amountToMinor('90071992547409.91', 2)).toBe(9007199254740991)
})
