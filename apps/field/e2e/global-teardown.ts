import { cleanupE2eRecords } from './support/db'

/**
 * Leave the database exactly as the suite found it. The seeded figures are
 * part of the specification and rls_test.sql asserts them, so records created
 * here must not survive the run.
 */
export default function globalTeardown() {
  const result = cleanupE2eRecords()
  if (!result.ran) {
    throw new Error(`[e2e] post-run cleanup failed: ${result.reason}`)
  }
}
