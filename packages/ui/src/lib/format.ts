/** Basis points (10000 = 100%) as a percentage; the decimal string is built exactly. */
export function formatShare(bp: number, locale: string): string {
  const digits = String(bp).padStart(5, "0")
  const fraction = `${digits.slice(0, -4)}.${digits.slice(-4)}`
  return new Intl.NumberFormat(locale, {
    style: "percent",
    maximumFractionDigits: 2,
    // String input keeps it exact (Intl.NumberFormat v3); TS's lib types lack the overload.
  }).format(fraction as unknown as number)
}

/** An API date (`YYYY-MM-DD`) in words. Read and shown as UTC so it never moves a day. */
export const formatDay = (day: string, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    dateStyle: "long",
    timeZone: "UTC",
  }).format(new Date(`${day}T00:00:00Z`))
