/**
 * Whether `n` fits a `numeric(p, places)` column's scale without rounding.
 *
 * Postgres does not reject a value past a numeric column's scale — it rounds
 * it silently (2.25 into `numeric(3,1)` is stored as 2.3). A form that sends
 * such a value changes the user's figure without telling them, so the forms
 * refuse it instead. This is the column's shape, not a business rule.
 *
 * Judged on the number actually sent, so `'2.250'` is 2.25 and fits two places.
 */
export function hasAtMostDecimals(n: number, places: number): boolean {
  if (!Number.isFinite(n)) return false
  return Number(n.toFixed(places)) === n
}
