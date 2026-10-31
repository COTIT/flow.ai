import { describe, expect, test } from 'vitest'

import { validatePeopleSearch } from '@/features/officer/peopleSearch'

/**
 * Same contract as `validateRequestSearch`: the URL is untrusted text, and a
 * value that cannot be honoured is dropped so the screen degrades to
 * unfiltered rather than erroring.
 */
describe('validatePeopleSearch', () => {
  test('keeps a real name search and a real verification filter', () => {
    expect(validatePeopleSearch({ q: 'Neema', verification: 'verified' })).toEqual({
      q: 'Neema',
      verification: 'verified',
    })
  })

  test('accepts every value the enum actually has', () => {
    for (const v of ['unverified', 'pending', 'verified', 'disputed']) {
      expect(validatePeopleSearch({ verification: v }).verification).toBe(v)
    }
  })

  test('drops an unknown verification rather than forwarding it', () => {
    // Forwarded, this comes back from PostgREST as
    // "invalid input value for enum verification_status".
    expect(validatePeopleSearch({ verification: 'not-a-status' })).toEqual({})
  })

  test('drops non-string values', () => {
    expect(validatePeopleSearch({ q: 42, verification: ['verified'] })).toEqual({})
  })

  test('trims a search, and a whitespace-only search is not a search', () => {
    expect(validatePeopleSearch({ q: '  Neema  ' }).q).toBe('Neema')
    expect(validatePeopleSearch({ q: '   ' })).toEqual({})
    expect(validatePeopleSearch({ q: '' })).toEqual({})
  })

  test('caps an absurdly long search rather than sending it', () => {
    const q = validatePeopleSearch({ q: 'a'.repeat(500) }).q
    expect(q).toHaveLength(100)
  })

  test('an empty search is the unfiltered view', () => {
    expect(validatePeopleSearch({})).toEqual({})
  })

  // The filters are independent: one bad value must not discard the other.
  test('a bad filter does not take a good one with it', () => {
    expect(validatePeopleSearch({ q: 'Neema', verification: 'nonsense' })).toEqual({
      q: 'Neema',
    })
  })
})
