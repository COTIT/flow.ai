import { DISPLAY_TIMEZONE } from '@/lib/format'

/**
 * Calendar days for the survey screens, in Tanzanian time.
 *
 * app_redemption_log and app_redemption_totals take plain `date`s and compare
 * them against `redeemed_at at time zone 'Africa/Dar_es_Salaam'`, so the range
 * the screen sends has to be Tanzanian days too — a UTC "today" is yesterday
 * for three hours every night.
 */

const PLAIN_DATE = /^(\d{4})-(\d{2})-(\d{2})$/

// en-CA formats a date as YYYY-MM-DD, which is the shape the RPCs take.
const darDay = new Intl.DateTimeFormat('en-CA', {
  timeZone: DISPLAY_TIMEZONE,
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
})

/** Today's date in Dar es Salaam. */
export function darToday(now: Date = new Date()): string {
  return darDay.format(now)
}

/** A plain date moved by whole days, with no timezone involved. */
export function addDays(date: string, days: number): string {
  const match = PLAIN_DATE.exec(date)
  if (!match) return date
  const [, year, month, day] = match
  const moved = new Date(Date.UTC(Number(year), Number(month) - 1, Number(day) + days))
  return moved.toISOString().slice(0, 10)
}

/** Today in Dar es Salaam back seven days. */
export function defaultRedemptionRange(now: Date = new Date()): { from: string; to: string } {
  const to = darToday(now)
  return { from: addDays(to, -7), to }
}

/** A real calendar day in YYYY-MM-DD form — 2026-02-31 is not one. */
function isPlainDate(value: unknown): value is string {
  if (typeof value !== 'string') return false
  const match = PLAIN_DATE.exec(value)
  if (!match) return false
  return addDays(value, 0) === value
}

export interface RedemptionSearch {
  from?: string
  to?: string
}

/**
 * The redemptions date range lives in the URL, so it arrives as untrusted
 * text. Anything that is not a date is dropped and the default range applies.
 */
export function validateRedemptionSearch(search: Record<string, unknown>): RedemptionSearch {
  const out: RedemptionSearch = {}
  if (isPlainDate(search.from)) out.from = search.from
  if (isPlainDate(search.to)) out.to = search.to
  return out
}

/**
 * "Closes on" is a day; `closes_at` is a moment. The survey stays open for the
 * whole of the chosen day in Tanzania. Dar es Salaam keeps +03:00 all year.
 */
export function closesOnToTimestamp(day: string): string | null {
  if (day === '') return null
  return `${day}T23:59:59+03:00`
}

/** A stored timestamp, as the Tanzanian day a date input shows. */
export function timestampToDarDate(iso: string | null): string {
  if (!iso) return ''
  const parsed = new Date(iso)
  if (Number.isNaN(parsed.getTime())) return ''
  return darDay.format(parsed)
}
