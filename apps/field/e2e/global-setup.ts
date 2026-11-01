import { cleanupE2eRecords } from './support/db'

/**
 * Clean slate before the suite. A previous run that crashed mid-test would
 * otherwise leave marked records behind and skew the seeded counts.
 *
 * Deliberately NOT warming the dev server. That was tried for QA #33 and the
 * measurement disproved the theory behind it: a cold first load is 1,386ms
 * against 325ms warm, so Vite's on-demand compile costs about a second — no
 * part of a ten-second timeout. The real cause was a transient auth failure,
 * fixed in `sessionQuery`.
 */
export default function globalSetup() {
  const result = cleanupE2eRecords()
  if (!result.ran) {
    throw new Error(`[e2e] pre-run cleanup failed: ${result.reason}`)
  }
}
