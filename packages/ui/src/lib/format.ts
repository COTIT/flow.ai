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

/** An API quantity ("16.00", "-1.00", "1.50") for reading: whole ones without decimals, others
 * as the API gives them, in the app's locale. The string goes in as-is, so nothing is rounded. */
export function formatQty(qty: string, locale: string): string {
  const whole = /^-?\d+(\.0*)?$/.test(qty)
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: whole ? 0 : 2,
    maximumFractionDigits: whole ? 0 : 20,
  }).format(qty as unknown as number)
}

/** An API date (`YYYY-MM-DD`) in words. Read and shown as UTC so it never moves a day. */
export const formatDay = (day: string, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    dateStyle: "long",
    timeZone: "UTC",
  }).format(new Date(`${day}T00:00:00Z`))

// A plain `YYYY-MM-DD` is a calendar day: read and shown as UTC so it never moves.
const instant = (iso: string) =>
  iso.length === 10 ? new Date(`${iso}T00:00:00Z`) : new Date(iso)
const zone = (iso: string) => (iso.length === 10 ? "UTC" : undefined)

/** An API date or timestamp as a short day ("Nov 4, 2026"), in the app's locale. */
export const formatDate = (iso: string, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeZone: zone(iso),
  }).format(instant(iso))

/** An API timestamp as day and time ("Nov 4, 2026, 6:12 PM"), in the app's locale. */
export const formatDateTime = (iso: string, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(iso))

/** An API timestamp's time of day ("18:12"), in the app's locale. */
export const formatTime = (iso: string, locale: string) =>
  new Intl.DateTimeFormat(locale, { timeStyle: "short" }).format(new Date(iso))
