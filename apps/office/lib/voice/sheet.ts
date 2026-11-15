import "server-only"
import {
  readSheet as readAny,
  SheetError,
  sheetType as typeOf,
  type Sheet,
} from "../imports/sheet"
import { VoiceError } from "./server"

export { SHEET_LIMIT, SHEET_TYPES, XLSX } from "../imports/sheet"
export type { Sheet, SheetRow } from "../imports/sheet"
export const IMPORT_PAGE = 50

/** Voice callers expect VoiceError; the shared parser throws SheetError. */
function voiced<T>(read: () => T): T {
  try {
    return read()
  } catch (error) {
    if (error instanceof SheetError)
      throw new VoiceError(error.message, error.status)
    throw error
  }
}

export const sheetType = (bytes: Uint8Array, declared: string) =>
  voiced(() => typeOf(bytes, declared))

export const readSheet = (bytes: Uint8Array, type: string): Sheet =>
  voiced(() => readAny(bytes, type))

const PRODUCT =
  /^(product|products|product name|name|item|item name|sku|code|product code|product number|product nr|barcode|artikel|naam|bidhaa|jina)$/
const PRICE = /(^|\s)(price|prijs|bei)($|\s)/

export type PriceRow = { line: number; key: string; price: string }
export type PricePage =
  | { needsColumns: string[] }
  | {
      columns: { product: string; price: string }
      from: number
      to: number
      total: number
      next: number | undefined
      rows: PriceRow[]
    }

/** One page of raw rows; reading the price and matching the product stay in the API. */
export function priceRows(
  sheet: Sheet,
  choice: { productColumn?: string; priceColumn?: string; fromRow?: number },
  size = IMPORT_PAGE
): PricePage {
  const norm = (s: string) => s.trim().toLowerCase().replace(/\s+/g, " ")
  const pick = (named: string | undefined, pattern: RegExp) => {
    if (named) {
      const index = sheet.columns.findIndex((c) => norm(c) === norm(named))
      if (index < 0)
        throw new VoiceError(
          `There is no column called "${named}". Columns: ${sheet.columns.join(", ")}.`,
          422
        )
      return index
    }
    const found = sheet.columns.flatMap((c, i) =>
      pattern.test(norm(c)) ? [i] : []
    )
    return found.length === 1 ? found[0]! : -1
  }
  const product = pick(choice.productColumn, PRODUCT)
  const price = pick(choice.priceColumn, PRICE)
  if (product < 0 || price < 0 || product === price)
    return { needsColumns: sheet.columns }
  const total = sheet.rows.length
  const from = choice.fromRow ?? 1
  if (!Number.isInteger(from) || from < 1 || from > total)
    throw new VoiceError(`Choose a start row from 1 to ${total}.`, 422)
  const to = Math.min(total, from + size - 1)
  return {
    columns: { product: sheet.columns[product]!, price: sheet.columns[price]! },
    from,
    to,
    total,
    next: to < total ? to + 1 : undefined,
    rows: sheet.rows.slice(from - 1, to).map((row) => ({
      line: row.line,
      key: (row.cells[product] ?? "").trim(),
      price: (row.cells[price] ?? "").trim(),
    })),
  }
}
