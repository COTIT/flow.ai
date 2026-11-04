// The API refuses with fixed English sentences and no code. shortcut: the till knows these by
// their text; switch to codes once the API sends them.
const KNOWN: Record<string, string> = {
  "This client_ref was used for another sale on this register.": "refReused",
  "No shift is open on this register: open the drawer first.": "noShift",
  "No shift is open on this register.": "noShift",
  "A shift is already open on this register.": "alreadyOpen",
}

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
  const known = said ? KNOWN[said] : undefined
  return known ? { title: t(known) } : { title: fallback, detail: said }
}
