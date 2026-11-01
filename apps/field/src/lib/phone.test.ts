import { describe, expect, test } from 'vitest'

import { normalizePhone } from '@/lib/phone'

/** Every spelling of one Tanzanian mobile number is one login. */
const SPELLINGS: Array<[string, string | null]> = [
  ['+255712345678', '+255712345678'],
  ['+255 712 345 678', '+255712345678'],
  ['255712345678', '+255712345678'],
  ['0712 345 678', '+255712345678'],
  ['0712-345-678', '+255712345678'],
  ['(0712) 345.678', '+255712345678'],
  ['712345678', '+255712345678'],
  ['0612345678', '+255612345678'],
  ['0812345678', null], // not a mobile prefix
  ['+1 555 0100', null],
  ['07123', null],
  ['', null],
]

describe('normalizePhone', () => {
  test.each(SPELLINGS)('%s → %s', (input, expected) => {
    expect(normalizePhone(input)).toBe(expected)
  })
})
