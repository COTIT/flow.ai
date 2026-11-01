/**
 * Route params are strings until something says otherwise — QA #3.
 *
 * Every `$id` route took its param straight into a query, so
 * `/officer/people/not-a-uuid` came back as
 * `invalid input syntax for type uuid: "not-a-uuid"`. A well-formed id that
 * matches nothing already behaves properly — RLS returns zero rows and the
 * screen says "not found" — so the fix is to recognise a malformed one before
 * the query and let it reach that same state, rather than translating the
 * parse failure afterwards.
 *
 * Search params were already handled this way (`/ops/tower?village=nonsense`
 * renders the village prompt); this brings params into line.
 */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function isUuid(value: string | null | undefined): boolean {
  return typeof value === 'string' && UUID.test(value)
}

/**
 * A v4 uuid, in a secure context or not.
 *
 * `crypto.randomUUID` is restricted to secure contexts. `localhost` is one, so
 * dev, Vitest and Playwright all had it; the LAN address a stakeholder opens
 * the demo on — `http://192.168.100.42:5173` — is not, and the register screen
 * threw `crypto.randomUUID is not a function` before rendering a field.
 *
 * `crypto.getRandomValues` carries no such restriction, so the fallback costs
 * nothing in quality: the same 122 random bits, version and variant set by
 * hand per RFC 4122. No `Math.random`, because the value becomes the RPC's
 * `client_ref` and two officers colliding on one would merge two farmers.
 */
export function newUuid(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()

  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = (bytes[6] & 0x0f) | 0x40
  bytes[8] = (bytes[8] & 0x3f) | 0x80

  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}
