import type { Link, PublicBlock } from "@genesy/api-client/model"

/** Batch QR code: 10 Crockford base32 characters (no I, L, O, U). */
export const isPublicCode = (code: string) =>
  /^[0-9A-HJKMNP-TV-Z]{10}$/.test(code)

/** Product ids are UUIDs; anything else would change the API path the server calls. */
export const isProductId = (id: string) =>
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)

/** The parts of a batch reply that carry signed file links. */
type WithLinks = {
  passport: {
    banner: Link | null
    certificates: { file: Link | null }[]
    story: PublicBlock[]
  }
  farmers: { photo: Link | null }[]
}

/** When the first signed link on the page stops working, or null when there are none. */
export function earliestLinkExpiry(lot: WithLinks): Date | null {
  const { passport, farmers } = lot
  const links = [
    passport.banner,
    ...passport.certificates.map((c) => c.file),
    ...passport.story.map((b) => (b.kind === "image" ? b.image : null)),
    ...farmers.map((f) => f.photo),
  ]
  const times = links.flatMap((l) => (l ? [Date.parse(l.expires_at)] : []))
  return times.length ? new Date(Math.min(...times)) : null
}

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
