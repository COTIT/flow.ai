/** A schedule's timing as the API sends it (ISO weekday 1 = Monday). */
export type ScheduleTiming = {
  frequency: string
  weekday: number | null
  day_of_month: number | null
  hour: number
}

/** ISO weekday `n` by name in `locale` (1 January 2024 was a Monday). */
export function weekdayName(n: number, locale: string) {
  return new Intl.DateTimeFormat(locale, {
    weekday: "long",
    timeZone: "UTC",
  }).format(new Date(Date.UTC(2024, 0, n)))
}

/** The message key and values for "Every Monday around 07:00". */
export function timingWords(s: ScheduleTiming, locale: string) {
  const time = `${String(s.hour).padStart(2, "0")}:00`
  const frequency = ["daily", "weekly", "monthly"].includes(s.frequency)
    ? s.frequency
    : "daily"
  const day =
    frequency === "weekly" && s.weekday
      ? weekdayName(s.weekday, locale)
      : String(s.day_of_month ?? "")
  return { key: `scheduled.every.${frequency}`, values: { day, time } }
}
