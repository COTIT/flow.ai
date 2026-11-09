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
