// The API refuses mostly with fixed English sentences: the till knows those by their text, and
// by `code` where the API sends one.
/** The API's 409 when a client_ref already holds another sale on this register. */
export const REUSED =
  "This client_ref was used for another sale on this register."

const KNOWN: Record<string, string> = {
  [REUSED]: "refReused",
  "No shift is open on this register: open the drawer first.": "noShift",
  "No shift is open on this register.": "noShift",
  "A shift is already open on this register.": "alreadyOpen",
  "Open the drawer before paying a cash refund.": "noShift",
  "Cash refunds must be a multiple of the cash rounding step.": "notStep",
  "A card refund needs the terminal's refund reference.": "needsReference",
  "Record a card refund only after the terminal confirmed it.":
    "needsConfirmed",
  "This sale was recorded before the accounting book became active; return it from the Office.":
    "beforeBook",
}

const CODES: Record<string, string> = {
  insufficient_stock: "insufficientStock",
  stale_document: "stale",
  unsupported_active_erp_operation: "notActive",
}

/** One API sentence (e.g. a field error) in the cashier's language when the till knows it. */
export const inWords = (said: string, t: (key: string) => string) =>
  KNOWN[said] ? t(KNOWN[said]) : said

/** The API's machine code for a refused call, if it sent one. */
export const apiCode = (e: unknown): string | undefined =>
  (e as { response?: { data?: { code?: unknown } } } | null)?.response?.data
    ?.code as string | undefined

/** The API's own (English) words for a refused call, if it gave some. */
export const apiMessage = (e: unknown): string | undefined =>
  typeof e === "string"
    ? e
    : (e as { response?: { data?: { message?: string } } } | null)?.response
        ?.data?.message

/**
 * A refused call in the cashier's language: a known refusal in its own words, else `fallback`
 * with the API's English kept as the detail. `t` reads the till's `refusals` messages.
 */
export function refusal(
  e: unknown,
  t: (key: string) => string,
  fallback: string
): { title: string; detail?: string } {
  const said = apiMessage(e)
  const code = apiCode(e)
  const known = (code && CODES[code]) || (said ? KNOWN[said] : undefined)
  return known ? { title: t(known) } : { title: fallback, detail: said }
}
