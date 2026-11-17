import type { Sheet } from "./sheet"

/** Columns the API accepts per profile (apps/api imports::profiles); the API stays the authority. */
export const FIELDS = {
  products: [
    "name",
    "price",
    "barcode",
    "ean13",
    "reference",
    "description",
    "unit",
  ],
  relations: [
    "type",
    "company_name",
    "alias",
    "coc_number",
    "vat_number",
    "email",
    "website",
    "phone_number",
    "default_language",
    "address",
    "address2",
    "house_number",
    "zipcode",
    "city",
    "state",
    "country",
  ],
} as const

export type Profile = keyof typeof FIELDS
export type Mapping = Partial<Record<string, number>>
export type ImportRow = {
  row_number: number
  external_key?: string
  fields: Record<string, string>
}

const norm = (s: string) =>
  s
    .trim()
    .toLowerCase()
    .replace(/[\s-]+/g, "_")

/** Preselect a column whose header names a field exactly; anything else stays unmapped. */
export function autoMapping(columns: string[], profile: Profile): Mapping {
  const mapping: Mapping = {}
  for (const field of FIELDS[profile]) {
    const at = columns.findIndex((c) => norm(c) === field)
    if (at >= 0) mapping[field] = at
  }
  return mapping
}

/** Import rows from the parsed sheet: mapped, non-empty cells only; `key` is the update key column. */
export function toRows(
  sheet: Sheet,
  mapping: Mapping,
  key?: number
): ImportRow[] {
  return sheet.rows.map((row, i) => {
    const fields: Record<string, string> = {}
    for (const [field, at] of Object.entries(mapping)) {
      const cell = at === undefined ? "" : (row.cells[at] ?? "")
      if (cell !== "") fields[field] = cell
    }
    const external = key === undefined ? "" : (row.cells[key] ?? "")
    return external
      ? { row_number: i + 1, external_key: external, fields }
      : { row_number: i + 1, fields }
  })
}

/** The API takes at most 20 rows per chunk. */
export function chunkRows<T>(rows: T[], size = 20): T[][] {
  const out: T[][] = []
  for (let i = 0; i < rows.length; i += size) out.push(rows.slice(i, i + size))
  return out
}

/** SHA-256 of the file, lowercase hex (the batch's `file_hash`). */
export async function fileHash(bytes: ArrayBuffer) {
  const digest = await crypto.subtle.digest("SHA-256", bytes)
  return Array.from(new Uint8Array(digest), (b) =>
    b.toString(16).padStart(2, "0")
  ).join("")
}

/** A batch as `imports_get` returns it. */
export type Batch = {
  id: string
  profile: Profile
  filename: string
  expected_rows: number
  mapping: { operation?: "create" | "update"; match_key?: string }
  state: "uploading" | "validated" | "applying" | "completed" | "failed"
  version: number
  validation_fingerprint: string | null
  counts: Record<
    "staged" | "valid" | "invalid" | "applied" | "failed" | "received",
    number
  >
  allowed_actions: string[]
  created_at: string
}

/** One row as `imports_rows` returns it. */
export type BatchRow = {
  id: string
  row_number: number
  external_key: string | null
  fields: Record<string, string>
  status: "staged" | "valid" | "invalid" | "applied" | "failed"
  issues: { code: string; message: string }[]
}

/** Badge tone per batch or row status. */
export const TONES = {
  validated: "teal",
  valid: "teal",
  completed: "teal",
  applied: "teal",
  applying: "gold",
  uploading: "gold",
  staged: "slate",
  invalid: "red",
  failed: "red",
} as const

/** The native right an apply needs besides `import.apply` (mirrors the API's check). */
export const nativeRight = (b: Pick<Batch, "profile" | "mapping">) =>
  b.profile === "relations"
    ? "relation.create"
    : b.mapping.operation === "update"
      ? "product.update"
      : "product.create"
