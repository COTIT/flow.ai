import { describe, expect, test } from 'vitest'

import { loginEmailForPhone, loginEmailFor, normalizePhone } from '@/lib/phone'

/**
 * The same cases `app_normalize_phone` answers in
 * supabase/migrations/20260929090001_farmer_login.sql. The phone is the login
 * name, so the browser and the database must agree on every spelling of it.
 */
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

describe('loginEmailForPhone', () => {
  test('maps a phone to the hidden login address the database created', () => {
    expect(loginEmailForPhone('0712 345 678')).toBe('255712345678@farmers.ruaha360.test')
  })

  test('has no address for something that is not a phone', () => {
    expect(loginEmailForPhone('not a phone')).toBeNull()
  })
})

describe('loginEmailFor', () => {
  test('an email is used as typed, trimmed', () => {
    expect(loginEmailFor('  ops@demo.ruaha360.test ')).toBe('ops@demo.ruaha360.test')
  })

  test('a phone becomes its login address', () => {
    expect(loginEmailFor('+255 712 345 678')).toBe('255712345678@farmers.ruaha360.test')
  })

  test('neither an email nor a phone gives nothing', () => {
    expect(loginEmailFor('neema')).toBeNull()
  })
})
