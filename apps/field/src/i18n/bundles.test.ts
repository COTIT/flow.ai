import { beforeEach, describe, expect, test } from 'vitest'

import en from '@/i18n/en/common.json'
import sw from '@/i18n/sw/common.json'
import flags from '@/i18n/sw/flags.json'
import sealed from '@/i18n/sw/en-source.json'
import reviewed from '@/i18n/sw/reviewed.json'
import {
  SURFACE_OF,
  englishFingerprint,
  flatten,
  reviewedKeysOf,
  validateReviewed,
  validateSwahili,
  wordingViolations,
} from '../../scripts/i18n-handover.mjs'

const i18n = (await import('@/i18n')).default

const EN = flatten(en) as Record<string, string>
const SW = flatten(sw) as Record<string, string>

beforeEach(async () => {
  await i18n.changeLanguage('en')
})

/**
 * The bundles, as a fence rather than as a driver.
 *
 * Until 29 Sep 2026 the rule was absolute: no Swahili the product owner had
 * not had a native reviewer supply. The owner then approved a DRAFT Swahili so
 * the demo works for people who do not read English. What stays absolute is
 * honesty about it: a string is `reviewed` only when `reviewed.json` says a
 * named person reviewed it, and everything else is a draft. These tests fail
 * on the day the bundle stops being complete, malformed, or dishonest.
 */
describe('the Swahili bundle is complete and well formed', () => {
  test('every Swahili key exists in English, so none is orphaned', () => {
    for (const key of Object.keys(SW)) expect(EN).toHaveProperty(key)
  })

  test('no Swahili value is blank, which would render as nothing at all', () => {
    for (const [key, value] of Object.entries(SW)) {
      expect(value.trim(), key).not.toBe('')
    }
  })

  // Placeholders that change, and a plural with only one half, break at render.
  test('placeholders and plural variants match the English', () => {
    expect(validateSwahili(en, sw)).toEqual([])
  })

  // CLAUDE.md: Farmer and Officer surfaces ship complete Swahili. The required
  // surface is the farmer, the officer, and the chrome both of them render.
  test('every required-surface key has Swahili', () => {
    const missing = Object.keys(EN).filter(
      (key) => SURFACE_OF(key) === 'required' && !(key in SW),
    )
    expect(missing, `${missing.length} required strings have no Swahili`).toEqual([])
  })

  // Ops and Tower may ship English per CLAUDE.md, but the draft now covers them
  // too, so a new string that lands without Swahili is caught here and not by
  // a farmer or an ops user seeing English in a Swahili session.
  test('every key has Swahili, ops and Tower included', () => {
    const missing = Object.keys(EN).filter((key) => !(key in SW))
    expect(missing, `${missing.length} strings have no Swahili`).toEqual([])
  })

  // A value identical to its English is an untranslated copy — the exact thing
  // this fence exists to catch — unless it has nothing to translate.
  test('no string is an untranslated copy of the English', () => {
    // Product names and symbols. "Control Tower" is what the product calls its
    // ops dashboard, the same in both languages.
    const NOTHING_TO_TRANSLATE = new Set([
      'Ruaha',
      'QR',
      'PIN',
      'GPS',
      'kW',
      'kWh',
      'TZS',
      'OK',
      'Control Tower',
      // A sales-channel acronym, shown as written.
      'AFM',
    ])
    const copies = Object.entries(SW)
      .filter(([key, value]) => value === EN[key])
      .filter(([key, value]) => key !== 'language.sw' && !NOTHING_TO_TRANSLATE.has(value))
      .map(([key]) => key)
    expect(copies).toEqual([])
  })
})

/**
 * A Swahili string is written from the English of the day. When the English
 * changes afterwards, the Swahili silently says the old thing: the tour's
 * welcome text once promised "seven short stops" in Swahili after the English
 * had stopped saying so. `en-source.json` records a fingerprint of the English
 * each string was written from, so a changed English fails here until someone
 * re-translates it and runs \`pnpm i18n:seal\`.
 */
describe('no Swahili string is stale', () => {
  test('every Swahili string has a recorded English source', () => {
    const unsealed = Object.keys(SW).filter((key) => !(key in sealed))
    expect(unsealed, 'run pnpm i18n:seal after translating').toEqual([])
  })

  test('and the English has not changed since it was translated', () => {
    const stale = Object.keys(SW).filter(
      (key) => key in sealed && (sealed as Record<string, string>)[key] !== englishFingerprint(EN[key]),
    )
    expect(stale, 'the English changed: re-translate, then pnpm i18n:seal').toEqual([])
  })

  test('nothing is sealed for a string that no longer exists', () => {
    const orphans = Object.keys(sealed).filter((key) => !(key in EN))
    expect(orphans).toEqual([])
  })
})

describe('the Swahili bundle is honest about what has been reviewed', () => {
  // A flag on a string that does not exist is a stale doubt, and the reviewer
  // would go looking for it.
  test('every flag is on a string that has Swahili', () => {
    for (const key of Object.keys(flags)) expect(SW, key).toHaveProperty(key)
  })

  test('every flag says why', () => {
    for (const [key, reason] of Object.entries(flags)) {
      expect(String(reason).trim(), key).not.toBe('')
    }
  })

  test('every reviewed batch names a reviewer and a date, and real strings', () => {
    expect(validateReviewed(reviewed, sw)).toEqual([])
  })

  // Anything not on the list is a draft, and the list can only grow by someone
  // putting their name on a batch.
  test('the two language names are the only reviewed strings so far', () => {
    expect([...reviewedKeysOf(reviewed)].sort()).toEqual(['language.en', 'language.sw'])
  })
})

/**
 * CLAUDE.md: a survey incentive is "a fixed cash amount per household per
 * survey, paid at the office. Never earnings, wallet, balance or payment."
 *
 * The forbidden list is DERIVED from the English rule, not taken from a
 * Tanzanian source. It exists to make a reviewer look at a string, not to
 * certify that the rest is right.
 */
describe('the incentive is never described as earnings, a wallet, a balance or a payment', () => {
  const INCENTIVE_NAMESPACES = [
    'surveys',
    'voucher',
    'redeem',
    'auditTrail',
    'surveyStatus',
    'voucherStatus',
    'farmerLogin',
  ]
  const FORBIDDEN = /\b(mapato|pochi|salio|malipo|mshahara)\b/i

  test('no incentive string uses those words', () => {
    expect(wordingViolations(sw, INCENTIVE_NAMESPACES, FORBIDDEN)).toEqual([])
  })

  // The tour explains the incentive in the farmer's, officer's and ops' own
  // words. Its keys live under `tour`, so they are picked out by what the
  // ENGLISH is about. English that itself names "payment", as the opportunity
  // disclaimer does, is not about the incentive and is left out.
  test('nor does any tour copy about the incentive or the voucher', () => {
    const offenders = Object.entries(SW)
      .filter(([key]) => key.startsWith('tour.'))
      .filter(([key]) => /incentive|voucher|redeem/i.test(EN[key] ?? ''))
      .filter(([key]) => !/earning|wallet|balance|payment|salary|wage/i.test(EN[key] ?? ''))
      .filter(([, value]) => FORBIDDEN.test(value))
      .map(([key, value]) => `${key}: "${value}"`)
    expect(offenders).toEqual([])
  })
})

/**
 * A key missing from sw must still render English, never its own path —
 * `register.harvestKg` where a label belongs is worse than either language.
 * The bundle is complete for the required surface, so the fallback is proven
 * with a key that exists only in English.
 */
describe('Swahili falls back to English cleanly', () => {
  test('a key only English has resolves to the English wording', async () => {
    i18n.addResource('en', 'common', '__probe.onlyEnglish', 'Only in English')
    await i18n.changeLanguage('sw')

    expect(i18n.t('__probe.onlyEnglish')).toBe('Only in English')
  })

  test('every English key resolves under sw, none to its own path', async () => {
    await i18n.changeLanguage('sw')

    const raw = Object.keys(EN).filter((key) => i18n.t(key) === key)
    expect(raw, 'these keys would render as their own path').toEqual([])
  })

  test('the two attested strings really are Swahili', async () => {
    await i18n.changeLanguage('sw')
    expect(i18n.t('language.en')).toBe('Kiingereza')
  })

  test('and English is unaffected by any of it', async () => {
    await i18n.changeLanguage('en')
    expect(i18n.t('language.en')).toBe('English')
  })
})

describe('the English bundle is complete in itself', () => {
  test('no key is blank', () => {
    for (const [key, value] of Object.entries(EN)) {
      expect(String(value).trim(), key).not.toBe('')
    }
  })

  // A placeholder that loses its braces renders the literal text, which is how
  // "Stored as {{value}}" becomes "Stored as value" in front of a user.
  test('no interpolation placeholder is malformed', () => {
    for (const [key, value] of Object.entries(EN)) {
      const braces = String(value).match(/\{+|\}+/g) ?? []
      for (const run of braces) expect(run.length, `${key}: ${value}`).toBe(2)
    }
  })

  // Reference data — crop, equipment and category names — is translated in the
  // DATABASE via name_en / name_sw. Those rows are created at runtime and a
  // repo file cannot translate them.
  test('holds no reference data that belongs in the database', () => {
    for (const key of Object.keys(EN)) {
      expect(key).not.toMatch(/^(crops|equipmentNames|categories)\./)
    }
  })
})
