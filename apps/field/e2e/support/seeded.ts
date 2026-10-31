import { test } from '@playwright/test'

import { cleanupE2eRecords } from './db'

/**
 * Declares that a spec asserts SEEDED figures.
 *
 * The Tower aggregates everything, and several specs create real records, so a
 * spec asserting CLAUDE.md's headline numbers — 12,000 / 6,400 / 5,600 /
 * 62.2% / 10.800 / 489.200 — depends on the database holding only seeded data.
 * Without this, whether such a spec passes depends on the alphabetical order
 * its file happens to run in, which is not a property worth relying on.
 *
 * Removes only marked records, so it restores the seeded state without
 * touching anything the seed created.
 */
export function assertsSeededFigures() {
  test.beforeAll(() => {
    const result = cleanupE2eRecords()
    if (!result.ran) {
      throw new Error(`[e2e] could not restore seeded state: ${result.reason}`)
    }
  })
}
