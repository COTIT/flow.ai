/**
 * Farmer phone numbers. A farmer signs in with the phone number their officer
 * registered; the Genesy API normalises it again on sign-in, so this copy only
 * decides, before a round trip, whether the login field holds a phone at all.
 */

/** Tanzanian mobile numbers as `+255` plus nine digits, or null. */
export function normalizePhone(input: string): string | null {
  let digits = input.replace(/[\s().-]/g, '')
  if (/^\+255\d{9}$/.test(digits)) digits = digits.slice(4)
  else if (/^255\d{9}$/.test(digits)) digits = digits.slice(3)
  else if (/^0\d{9}$/.test(digits)) digits = digits.slice(1)
  return /^[67]\d{8}$/.test(digits) ? `+255${digits}` : null
}
