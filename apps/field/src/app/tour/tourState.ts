import type { Surface } from '@/app/membership'

/**
 * Whether a tour has already been given.
 *
 * A per-device convenience, so `localStorage` and nowhere else. It is NOT a
 * column: the schema has none, adding one would be a migration, and a missing
 * column in this schema is usually deliberate (CLAUDE.md). Nothing here is a
 * security decision — the worst outcome of getting it wrong is a tour the user
 * has already seen, or one they have to start themselves.
 *
 * Keyed by surface AND user: a demo laptop signs in as an officer, then ops,
 * then a farmer inside an hour, and each of those is somebody's first time.
 */
export const TOUR_STORAGE_KEY = 'ruaha360:tours-seen'

const id = (surface: Surface, userId: string) => `${surface}:${userId}`

/**
 * A browser in private mode, or one told to block site data, THROWS on access
 * rather than returning nothing. A tour is the least important thing on the
 * screen and may never be the reason the app fails to render, so every read
 * falls back to "already seen" and every write is allowed to do nothing.
 */
function read(): string[] {
  try {
    const raw = localStorage.getItem(TOUR_STORAGE_KEY)
    if (!raw) return []
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((entry): entry is string => typeof entry === 'string')
  } catch {
    return []
  }
}

export function hasSeenTour(surface: Surface, userId: string | null | undefined): boolean {
  // No signed-in user is not a first time; it is nobody's time.
  if (!userId) return true
  try {
    localStorage.getItem(TOUR_STORAGE_KEY)
  } catch {
    return true
  }
  return read().includes(id(surface, userId))
}

export function markTourSeen(surface: Surface, userId: string | null | undefined): void {
  if (!userId) return
  const seen = read()
  if (seen.includes(id(surface, userId))) return
  try {
    localStorage.setItem(TOUR_STORAGE_KEY, JSON.stringify([...seen, id(surface, userId)]))
  } catch {
    // Nothing to do and nothing worth saying. The tour simply runs again.
  }
}
