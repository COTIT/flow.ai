import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'

import { TOUR_STORAGE_KEY, hasSeenTour, markTourSeen } from '@/app/tour/tourState'

/**
 * Whether a tour has already been given is a per-device convenience, so it
 * lives in `localStorage` and nowhere else. It is NOT a column: the schema has
 * none for it, inventing one would be a migration, and CLAUDE.md is explicit
 * that a missing column is usually deliberate.
 *
 * Keyed by surface AND user, because a demo laptop signs in as an officer, then
 * ops, then a farmer within the same hour, and each of those is a first time.
 *
 * Every read and write is wrapped: a browser in private mode, or one told to
 * block site data, THROWS on access rather than returning nothing. A tour is
 * the least important thing on the screen and may never be the reason the app
 * fails to render.
 */
const OFFICER = '80000000-0000-4000-8000-000000000003'

describe('remembering that a tour has been given', () => {
  beforeEach(() => localStorage.clear())

  test('a user who has never seen it, has not seen it', () => {
    expect(hasSeenTour('officer', OFFICER)).toBe(false)
  })

  test('marking it seen is remembered', () => {
    markTourSeen('officer', OFFICER)
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })

  test('each surface is its own first time', () => {
    markTourSeen('officer', OFFICER)
    expect(hasSeenTour('ops', OFFICER)).toBe(false)
    expect(hasSeenTour('farmer', OFFICER)).toBe(false)
  })

  test('and so is each user on a shared device', () => {
    markTourSeen('officer', OFFICER)
    expect(hasSeenTour('officer', '80000000-0000-4000-8000-000000000004')).toBe(false)
  })

  test('marking twice does not accumulate', () => {
    markTourSeen('officer', OFFICER)
    markTourSeen('officer', OFFICER)
    expect(JSON.parse(localStorage.getItem(TOUR_STORAGE_KEY)!)).toHaveLength(1)
  })

  // Nobody is signed in yet, or the session has no app_user row. Asking is
  // fine; the answer is that there is no first time to be had.
  test('an unknown user has no tour state either way', () => {
    expect(hasSeenTour('officer', null)).toBe(true)
    markTourSeen('officer', null)
    expect(localStorage.getItem(TOUR_STORAGE_KEY)).toBeNull()
  })
})

describe('storage that refuses to work', () => {
  const getItem = Storage.prototype.getItem
  const setItem = Storage.prototype.setItem

  beforeEach(() => {
    Storage.prototype.getItem = vi.fn(() => {
      throw new DOMException('denied', 'SecurityError')
    })
    Storage.prototype.setItem = vi.fn(() => {
      throw new DOMException('denied', 'SecurityError')
    })
  })

  afterEach(() => {
    Storage.prototype.getItem = getItem
    Storage.prototype.setItem = setItem
  })

  test('reads as "already seen", so a locked-down browser gets the app rather than a tour', () => {
    expect(() => hasSeenTour('officer', OFFICER)).not.toThrow()
    expect(hasSeenTour('officer', OFFICER)).toBe(true)
  })

  test('and writing is silent', () => {
    expect(() => markTourSeen('officer', OFFICER)).not.toThrow()
  })
})

describe('storage holding junk', () => {
  beforeEach(() => localStorage.clear())

  test.each(['not json', '{"seen":true}', '"a string"', '[1, 2, 3]'])('%s is ignored', (value) => {
    localStorage.setItem(TOUR_STORAGE_KEY, value)
    expect(() => hasSeenTour('officer', OFFICER)).not.toThrow()
    expect(hasSeenTour('officer', OFFICER)).toBe(false)
  })
})
