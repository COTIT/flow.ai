/**
 * Every record the browser suite creates is marked, so cleanup can find it
 * without guessing and can never touch a seeded demo record.
 *
 * The seeded figures are part of the specification — rls_test.sql asserts
 * exactly six Ilundo persons — so a registration test that left rows behind
 * would break the schema assertions on the next run.
 */
export const E2E_MARKER = 'E2E-'

/** A unique, marked family name for one test's records. */
export function markedName(): string {
  return `${E2E_MARKER}${Math.random().toString(36).slice(2, 10)}`
}
