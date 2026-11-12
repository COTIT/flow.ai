export type DiffRow = {
  field: string
  /** The API's own name for the row (e.g. a warehouse), shown as is. */
  label?: string
  before: string | null
  after: string
  /** Signed basis points from the API; null when it sent none (e.g. old value 0). */
  change: number | null
  warning: boolean
}

type Obj = Record<string, unknown>
const obj = (v: unknown): v is Obj => !!v && typeof v === "object" && !Array.isArray(v)
const DECIMAL = /^-?\d+(\.\d+)?$/

/** A value as given; an exact decimal money string (sub-cent, never rounded) gets its code. */
const shown = (v: unknown, currency: unknown) => {
  const text = typeof v === "string" ? v : typeof v === "number" && Number.isFinite(v) ? String(v) : null
  return text !== null && typeof currency === "string" && DECIMAL.test(text) ? `${text} ${currency}` : text
}

/** Request fields a person reads as is; anything else (ids, codes, nested data) stays under
 * "More details". Each has a `voice.reviewFields` label. */
const PLAIN = new Set(["name", "title", "first_name", "last_name", "company_name", "email", "phone",
  "description", "note", "reference", "quantity", "amount", "price", "date", "due_date"])
const MONEY = new Set(["amount", "price"])
const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i

/** The request's own plain values as new-value rows, for writes the API sent no rows for.
 * Money only from an exact decimal string or whole number, never a float. */
function requested(body: unknown, currency: unknown): DiffRow[] {
  if (!obj(body)) return []
  return Object.entries(body).flatMap(([field, v]) => {
    if (!PLAIN.has(field)) return []
    const exact = typeof v === "string" ? v.trim() !== "" && !UUID.test(v) : Number.isSafeInteger(v)
    const plain = MONEY.has(field) ? exact : exact || (typeof v === "number" && Number.isFinite(v))
    const after = plain ? shown(v, MONEY.has(field) ? currency : undefined) : null
    return after === null ? [] : [{ field, before: null, after, change: null, warning: false }]
  })
}

/** What the review card leads with, all from the API (`reviewFacts` already formatted money):
 * changed fields, the record a delete removes, warnings and impact counts. Nothing listed →
 * null, and the card stays as it was: old values are never guessed. */
export function reviewSummary(details: Obj) {
  const rows: DiffRow[] = []
  const changes = obj(details.current) ? details.current.changes : undefined
  if (Array.isArray(changes))
    for (const item of changes) {
      if (!obj(item)) return null
      const after = shown(item.after, item.currency)
      if (typeof item.field !== "string" || after === null) return null
      rows.push({
        field: item.field,
        ...(typeof item.label === "string" && item.label.trim() ? { label: item.label } : {}),
        before: shown(item.before, item.currency),
        after,
        change: Number.isSafeInteger(item.change_bp) ? (item.change_bp as number) : null,
        warning: item.warning === "large_change",
      })
    }
  const r = details.record
  // Only the `{type, label}` shape names a record here; a label may be null, never an id.
  const record =
    obj(r) && typeof r.type === "string" && "label" in r && (r.label === null || typeof r.label === "string")
      ? { type: r.type, label: r.label as string | null }
      : null
  // Only a record line folds the request away; without one the card already shows it whole.
  if (!rows.length && record)
    rows.push(...requested(details.changes, obj(details.current) ? details.current.currency : undefined))
  const warnings = Array.isArray(details.warnings)
    ? details.warnings.filter((w): w is string => typeof w === "string" && w.trim() !== "")
    : []
  const counts = obj(details.impact) ? details.impact : {}
  const impact = Object.entries(counts).flatMap(([key, count]) =>
    Number.isSafeInteger(count) ? [{ key, count: count as number }] : []
  )
  if (!rows.length && !record && !warnings.length && !impact.length) return null
  return {
    record,
    warnings,
    impact,
    ...(typeof counts.effect === "string" ? { effect: counts.effect } : {}),
    rows,
  }
}
