import type { Link, PublicBlock } from "@genesy/api-client/model"

/** Batch QR code: 10 Crockford base32 characters (no I, L, O, U). */
export const isPublicCode = (code: string) =>
  /^[0-9A-HJKMNP-TV-Z]{10}$/.test(code)

/** Product and order ids are UUIDs; anything else would change the API path the server calls. */
export const isUuid = (id: string) =>
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id)

/** The parts of a batch reply that carry signed file links. */
type WithLinks = {
  passport: {
    banner: Link | null
    certificates: { file: Link | null }[]
    story: PublicBlock[]
  }
}

/** When the first signed link on the page stops working, or null when there are none. */
export function earliestLinkExpiry(lot: WithLinks): Date | null {
  const { passport } = lot
  const links = [
    passport.banner,
    ...passport.certificates.map((c) => c.file),
    ...passport.story.map((b) => (b.kind === "image" ? b.image : null)),
  ]
  const times = links.flatMap((l) => (l ? [Date.parse(l.expires_at)] : []))
  return times.length ? new Date(Math.min(...times)) : null
}
