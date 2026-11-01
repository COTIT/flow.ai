import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'
import { afterEach, describe, expect, test } from 'vitest'

import en from '@/i18n/en/common.json'
import { DB_MESSAGE_RULES, translateDbMessage } from '@/lib/dbMessages'
import { flatten } from '../../scripts/i18n-handover.mjs'

const i18n = (await import('@/i18n')).default
const EN = flatten(en) as Record<string, string>

afterEach(async () => {
  await i18n.changeLanguage('en')
})

/**
 * business-rules §9 says the schema's own messages are shown verbatim, because
 * the numbers and names in them are the part worth reading. That stays the
 * default. What changes is that a message the app can name exactly is shown in
 * the user's language, with every value it carried, and one it cannot name is
 * still shown as written.
 */
describe('translateDbMessage', () => {
  test('a fixed sentence maps to a key with no values', () => {
    expect(translateDbMessage('this survey is closed')).toEqual({
      key: 'dbError.surveyClosed',
      values: {},
    })
  })

  // The position is what tells a farmer which question to fix.
  test('a sentence with a value keeps the value', () => {
    expect(translateDbMessage('question 3 is required')).toEqual({
      key: 'dbError.questionRequired',
      values: { position: '3' },
    })
    expect(translateDbMessage('question 12: enter a number')).toEqual({
      key: 'dbError.questionEnterNumber',
      values: { position: '12' },
    })
  })

  test('the redeemed voucher keeps who and when', () => {
    expect(
      translateDbMessage('voucher already redeemed on 05 Sep 2026 14:30 by Salima Officer'),
    ).toEqual({
      key: 'dbError.voucherAlreadyRedeemed',
      values: { date: '5 Sep 2026, 14:30', name: 'Salima Officer' },
    })
  })

  // The database falls back to an English phrase when it has no name, which
  // must not be printed inside a Swahili sentence.
  test('and its English "another staff member" fallback is not passed through', () => {
    expect(
      translateDbMessage('voucher already redeemed on 05 Sep 2026 14:30 by another staff member'),
    ).toEqual({
      key: 'dbError.voucherAlreadyRedeemedByOther',
      values: { date: '5 Sep 2026, 14:30' },
    })
  })

  test('a void reason typed by an admin is passed through untouched', () => {
    expect(translateDbMessage('this voucher was voided: issued twice, by mistake')).toEqual({
      key: 'dbError.voucherVoided',
      values: { reason: 'issued twice, by mistake' },
    })
  })

  test('the expiry date follows the language, not the database', async () => {
    await i18n.changeLanguage('sw')
    expect(translateDbMessage('this voucher expired on 03 Mar 2026')?.values).toEqual({
      date: '3 Mac 2026',
    })
  })

  test('some messages are developer-facing and read as the generic error', () => {
    expect(translateDbMessage('an answer does not match any question in this survey')?.key).toBe(
      'error.unexpected',
    )
  })

  // An unfamiliar sentence is far more likely to be one of ours than noise, so
  // it is shown as written.
  test('a message it does not know is left for verbatim display', () => {
    expect(translateDbMessage('over-commitment: 4100.00 kg available')).toBeNull()
  })

  test('the whole message has to match, not a fragment of it', () => {
    expect(translateDbMessage('this survey is closed for maintenance')).toBeNull()
    expect(translateDbMessage('  this survey is closed  ')?.key).toBe('dbError.surveyClosed')
  })
})

describe('every rule points at real text', () => {
  // The messages live in the Genesy API's Rust sources.
  const apiDir = join(__dirname, '../../../api/src')
  const sources = readdirSync(apiDir, { recursive: true, encoding: 'utf8' })
    .filter((file) => file.endsWith('.rs'))
    .map((file) => readFileSync(join(apiDir, file), 'utf8'))
    .join('\n')

  test('the API sources are there to read', () => {
    expect(sources.length).toBeGreaterThan(100_000)
  })

  // A reworded message would otherwise stop matching in silence and fall back
  // to English, which is exactly the failure a Swahili farmer cannot diagnose.
  test.each(DB_MESSAGE_RULES.map((rule) => [rule.key, rule.source] as const))(
    '%s: %s is still in the API',
    (_key, source) => {
      expect(sources).toContain(source)
    },
  )

  test('every rule resolves to a key that exists in English', () => {
    for (const rule of DB_MESSAGE_RULES) expect(EN, rule.key).toHaveProperty(rule.key)
  })

  test('every interpolated value a key expects is supplied by its rule', () => {
    for (const rule of DB_MESSAGE_RULES) {
      const expected = [...(EN[rule.key] ?? '').matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort()
      const example = rule.example
      const result = translateDbMessage(example)
      expect(result, `${rule.key}: example does not match`).not.toBeNull()
      expect(Object.keys(result!.values ?? {}).sort(), rule.key).toEqual(expected)
    }
  })
})
