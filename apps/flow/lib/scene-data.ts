import type { FlowPeriod } from "@genesy/api-client/model"

/** Day bounds the sales routes take: `from` inclusive, `to` exclusive, workspace-local days. */
export type Range = { from: string; to: string }

const DAY = 86_400_000
const parse = (day: string) => Date.parse(`${day}T00:00:00Z`)
const iso = (ms: number) => new Date(ms).toISOString().slice(0, 10)
const shift = (day: string, days: number) => iso(parse(day) + days * DAY)

/** The workspace's calendar day at `now`; UTC when the zone is missing or unknown (never the browser's). */
export function localToday(now: Date, timeZone: string | null): string {
  try {
    if (timeZone)
      return new Intl.DateTimeFormat("en-CA", { timeZone }).format(now)
  } catch {
    // Unknown zone: fall through to UTC.
  }
  return now.toISOString().slice(0, 10)
}

/** How far `timeZone`'s wall clock is ahead of UTC at `ms`; 0 when the zone is missing or unknown. */
function offsetAt(ms: number, timeZone: string | null): number {
  if (!timeZone) return 0
  try {
    const p = Object.fromEntries(
      new Intl.DateTimeFormat("en-CA", {
        timeZone,
        hourCycle: "h23",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
      })
        .formatToParts(ms)
        .map((x) => [x.type, x.value])
    )
    const wall = Date.parse(
      `${p.year}-${p.month}-${p.day}T${p.hour}:${p.minute}:${p.second}Z`
    )
    return wall - Math.floor(ms / 1000) * 1000
  } catch {
    return 0
  }
}

/**
 * The instant `day` starts in the workspace zone (RFC 3339, UTC), for routes that take instants
 * (sales-summary); UTC midnight when the zone is missing or unknown.
 */
export function midnightIn(day: string, timeZone: string | null): string {
  const wall = parse(day)
  // Second pass uses the offset at the found instant, so a DST change that day is honoured.
  const first = wall - offsetAt(wall, timeZone)
  return new Date(wall - offsetAt(first, timeZone)).toISOString()
}

/** Monday of `day`'s week. */
const monday = (day: string) =>
  shift(day, -((new Date(parse(day)).getUTCDay() + 6) % 7))

export function periodRange(period: FlowPeriod, today: string): Range {
  const tomorrow = shift(today, 1)
  switch (period) {
    case "today":
      return { from: today, to: tomorrow }
    case "yesterday":
      return { from: shift(today, -1), to: today }
    case "this_week":
      return { from: monday(today), to: tomorrow }
    case "last_week":
      return { from: shift(monday(today), -7), to: monday(today) }
    case "last_7_days":
      return { from: shift(today, -6), to: tomorrow }
    case "this_month":
      return { from: `${today.slice(0, 8)}01`, to: tomorrow }
  }
}

/** The span a comparison sets the period against: the day before, the week before, the month before. */
export function comparePeriod(period: FlowPeriod, today: string): Range {
  const { from, to } = periodRange(period, today)
  if (period === "today" || period === "yesterday")
    return { from: shift(from, -1), to: from }
  if (period === "this_month") {
    const first = new Date(parse(from))
    first.setUTCMonth(first.getUTCMonth() - 1)
    const start = iso(first.getTime())
    const end = shift(start, (parse(to) - parse(from)) / DAY)
    // Last month is shorter on 29-31 March etc.: stop at the 1st, never count this month.
    return { from: start, to: end < from ? end : from }
  }
  return { from: shift(from, -7), to: shift(to, -7) }
}

/** A daily-sales range that holds every zone's "today", only to learn the workspace zone. */
export const probeRange = (now: Date): Range => {
  const day = now.toISOString().slice(0, 10)
  return { from: shift(day, -1), to: shift(day, 2) }
}

/** How long each source's answer counts as fresh (and how often a visible scene refetches). */
export const TTL = {
  orders: 30_000,
  attention: 60_000,
  sales: 120_000,
  series: 300_000,
  reports: 300_000,
} as const
