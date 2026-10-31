import { describe, expect, test } from 'vitest'

import { localisedField, localisedName, type LocalisedNames } from '@/lib/names'

const maize: LocalisedNames = { name_en: 'Maize', name_sw: 'Mahindi' }

/**
 * QA #31. Reference data — crops, equipment, categories — is translated in the
 * DATABASE, because those rows are created at runtime and a repo file cannot
 * translate them. Both columns come back from every query that needs a name,
 * and the choice between them happens at RENDER, never inside a `queryFn`:
 * a name chosen at fetch time is cached under a key about the row, and the
 * first locale to resolve wins for good.
 */
describe('localisedName', () => {
  test('gives the Swahili name in Swahili', () => {
    expect(localisedName(maize, 'sw')).toBe('Mahindi')
  })

  test('and the English name in anything else', () => {
    expect(localisedName(maize, 'en')).toBe('Maize')
    // i18next can report a region, and `sw-TZ` is still Swahili.
    expect(localisedName(maize, 'en-GB')).toBe('Maize')
  })

  test('treats a regional Swahili tag as Swahili', () => {
    expect(localisedName(maize, 'sw-TZ')).toBe('Mahindi')
  })

  test('falls back to English when the Swahili name is missing', () => {
    expect(localisedName({ name_en: 'Maize', name_sw: '' }, 'sw')).toBe('Maize')
    expect(localisedName({ name_en: 'Maize', name_sw: null }, 'sw')).toBe('Maize')
  })

  // A row that arrived without its names must not render "undefined" or crash
  // the screen around it. An empty label is a gap; "undefined" is a bug on
  // display.
  test('a missing row is an empty string, not a crash', () => {
    expect(localisedName(null, 'sw')).toBe('')
    expect(localisedName(undefined, 'en')).toBe('')
  })

  test('an undefined language is English', () => {
    expect(localisedName(maize, undefined)).toBe('Maize')
  })
})

describe('localisedField', () => {
  const survey = { title_en: 'Maize storage', title_sw: 'Uhifadhi wa mahindi' }

  test('picks the Swahili column in Swahili', () => {
    expect(localisedField(survey, 'title', 'sw-TZ')).toBe('Uhifadhi wa mahindi')
  })

  test('picks English otherwise', () => {
    expect(localisedField(survey, 'title', 'en')).toBe('Maize storage')
  })

  test('falls back to English while the Swahili is still awaiting a reviewer', () => {
    expect(localisedField({ title_en: 'Maize storage', title_sw: null }, 'title', 'sw')).toBe(
      'Maize storage',
    )
  })
})
