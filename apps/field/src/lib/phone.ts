/**
 * Farmer phone numbers, and the login name behind them.
 *
 * A farmer signs in with the phone number their officer registered. Supabase
 * signs in by email, so each farmer login has a hidden address derived from
 * the phone — created by `app_farmer_login_issue`, never shown, never mailed.
 *
 * `normalizePhone` mirrors `app_normalize_phone` in
 * supabase/migrations/20260929090001_farmer_login.sql: two spellings of one
 * number must be one login. The database is what stores the number; this copy
 * exists only because the address has to be known BEFORE there is a session
 * to ask the database with.
 */

const FARMER_LOGIN_DOMAIN = 'farmers.ruaha360.test'

/** Tanzanian mobile numbers as `+255` plus nine digits, or null. */
export function normalizePhone(input: string): string | null {
  let digits = input.replace(/[\s().-]/g, '')
  if (/^\+255\d{9}$/.test(digits)) digits = digits.slice(4)
  else if (/^255\d{9}$/.test(digits)) digits = digits.slice(3)
  else if (/^0\d{9}$/.test(digits)) digits = digits.slice(1)
  return /^[67]\d{8}$/.test(digits) ? `+255${digits}` : null
}

/** The hidden login address for a farmer's phone, or null. */
export function loginEmailForPhone(input: string): string | null {
  const phone = normalizePhone(input)
  return phone ? `${phone.slice(1)}@${FARMER_LOGIN_DOMAIN}` : null
}

/**
 * What to sign in with: staff type an email, farmers type a phone. Null when
 * the input is neither.
 */
export function loginEmailFor(input: string): string | null {
  const value = input.trim()
  if (value.includes('@')) return value
  return loginEmailForPhone(value)
}
