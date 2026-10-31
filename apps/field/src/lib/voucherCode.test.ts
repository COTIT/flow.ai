import { describe, expect, test } from 'vitest'

import { formatVoucherCode, normalizeVoucherCode, voucherQrPayload } from '@/lib/voucherCode'

/** Mirrors `app_voucher_normalize` in 20260929090003_surveys.sql. */
describe('normalizeVoucherCode', () => {
  test('a scanned QR payload yields the bare code', () => {
    expect(normalizeVoucherCode('R360V:K7QXM2PA9D')).toBe('K7QXM2PA9D')
  })

  test('a typed code tolerates case, spaces and the display dash', () => {
    expect(normalizeVoucherCode(' k7qxm-2pa9d ')).toBe('K7QXM2PA9D')
  })

  test('letters that Crockford base32 never uses are read as the digits they look like', () => {
    expect(normalizeVoucherCode('K7QXM-2PAOI')).toBe('K7QXM2PA01')
    expect(normalizeVoucherCode('L7QXM-2PA9D')).toBe('17QXM2PA9D')
  })

  test('anything that is not ten code symbols is not a code', () => {
    expect(normalizeVoucherCode('K7QXM2PA9')).toBeNull()
    expect(normalizeVoucherCode('K7QXM2PA9DZ')).toBeNull()
    expect(normalizeVoucherCode('https://example.com')).toBeNull()
    expect(normalizeVoucherCode('')).toBeNull()
  })
})

describe('formatVoucherCode', () => {
  test('groups the code in two halves for reading aloud', () => {
    expect(formatVoucherCode('K7QXM2PA9D')).toBe('K7QXM-2PA9D')
  })
})

describe('voucherQrPayload', () => {
  test('prefixes the code so a stray QR code is never mistaken for a voucher', () => {
    expect(voucherQrPayload('K7QXM2PA9D')).toBe('R360V:K7QXM2PA9D')
  })
})
