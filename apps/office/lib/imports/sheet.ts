import "server-only"
import { unzipSync } from "fflate"

export const XLSX =
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
export const SHEET_LIMIT = 2 * 1024 * 1024
export const SHEET_TYPES = [XLSX, "text/csv"] as const
const UNZIPPED_LIMIT = 20 * 1024 * 1024
const MAX_ENTRIES = 50
const MAX_ROWS = 500
const MAX_COLUMNS = 30
const MAX_CELL = 255
export const MAX_ROW_BYTES = 48 * 1024

/** A refusal with the HTTP status the caller should answer. */
export class SheetError extends Error {
  constructor(
    message: string,
    public readonly status = 400
  ) {
    super(message)
  }
}

export type SheetRow = { line: number; cells: string[] }
export type Sheet = { columns: string[]; rows: SheetRow[] }

const wrongType = () =>
  new SheetError(
    "Save the price list as .xlsx or .csv and share it again.",
    415
  )
const unreadable = () =>
  new SheetError(
    "This spreadsheet cannot be read. Save it again as .xlsx or .csv.",
    422
  )

/** Classify by bytes; Windows labels .csv as ms-excel, so text is csv whatever the label. */
export function sheetType(bytes: Uint8Array, declared: string) {
  const zip = bytes[0] === 0x50 && bytes[1] === 0x4b && bytes[2] === 3
  const ole = bytes[0] === 0xd0 && bytes[1] === 0xcf && bytes[2] === 0x11
  if (ole) throw wrongType()
  if (zip) {
    if (![XLSX, "application/octet-stream", ""].includes(declared))
      throw wrongType()
    return XLSX
  }
  if (declared === XLSX) throw wrongType()
  return "text/csv"
}

/** Bounded read of the first sheet; numbers stay the exact text Excel stored. */
export function readSheet(bytes: Uint8Array, type: string): Sheet {
  if (bytes.length > SHEET_LIMIT)
    throw new SheetError("Spreadsheets must not exceed 2 MiB.", 413)
  const raw = type === XLSX ? readXlsx(bytes) : readCsv(bytes)
  const rows = raw.filter((row) => row.cells.some((cell) => cell.trim()))
  const [header, ...data] = rows
  if (!header || !data.length)
    throw new SheetError("This spreadsheet has no rows to import.", 422)
  if (data.length > MAX_ROWS)
    throw new SheetError(
      `Spreadsheets can have at most ${MAX_ROWS} rows. Split the file and share each part.`,
      413
    )
  for (const row of rows) {
    if (row.cells.length > MAX_COLUMNS)
      throw new SheetError(
        `Spreadsheets can have at most ${MAX_COLUMNS} columns.`,
        413
      )
    if (row.cells.some((cell) => cell.length > MAX_CELL))
      throw new SheetError(
        `Row ${row.line} has a cell longer than ${MAX_CELL} characters.`,
        422
      )
  }
  rows.forEach(checkRowSize)
  return { columns: header.cells.map((c) => c.trim()), rows: data }
}

function readCsv(bytes: Uint8Array): SheetRow[] {
  let text: string
  try {
    text = new TextDecoder("utf-8", { fatal: true }).decode(bytes)
  } catch {
    throw new SheetError("Save the CSV file as UTF-8 and share it again.", 422)
  }
  text = text.replace(/^\uFEFF/, "")
  const first = text.split(/\r?\n/, 1)[0] ?? ""
  const count = (c: string) => first.split(c).length
  const delimiter = [";", "\t", ","].reduce((a, b) =>
    count(b) > count(a) ? b : a
  )
  const rows: SheetRow[] = []
  let cells: string[] = []
  let cell = ""
  let quoted = false
  let line = 1
  let start = 1
  const endCell = () => {
    cells.push(cell)
    cell = ""
  }
  const endRow = () => {
    endCell()
    rows.push({ line: start, cells })
    cells = []
    start = line
  }
  for (let i = 0; i < text.length; i++) {
    const c = text[i]!
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        cell += '"'
        i++
      } else if (c === '"') quoted = false
      else {
        if (c === "\n") line++
        cell += c
      }
    } else if (c === '"' && !cell) quoted = true
    else if (c === delimiter) endCell()
    else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++
      line++
      endRow()
    } else cell += c
  }
  if (cell || cells.length) endRow()
  return rows
}

function unzip(bytes: Uint8Array, wanted: (name: string) => boolean) {
  let entries = 0
  let total = 0
  let files: Record<string, Uint8Array>
  try {
    files = unzipSync(bytes, {
      // fflate inflates right after a true return into a buffer of the declared size, so limits gate here.
      filter(file) {
        entries++
        if (entries > MAX_ENTRIES || !wanted(file.name)) return false
        total += file.originalSize
        return total <= UNZIPPED_LIMIT
      },
    })
  } catch {
    throw unreadable()
  }
  if (entries > MAX_ENTRIES)
    throw new SheetError("This spreadsheet has too many parts to read.", 413)
  if (total > UNZIPPED_LIMIT)
    throw new SheetError("This spreadsheet is too large to read.", 413)
  return files
}

function xml(file: Uint8Array | undefined, required = true) {
  if (!file) {
    if (required) throw unreadable()
    return ""
  }
  if (file.length > UNZIPPED_LIMIT)
    throw new SheetError("This spreadsheet is too large to read.", 413)
  const text = new TextDecoder().decode(file)
  if (/<!DOCTYPE|<!ENTITY/i.test(text)) throw unreadable()
  return text
}

const ENTITIES: Record<string, string> = {
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
}
function decode(text: string) {
  return text.replace(/&(#x[0-9a-f]+|#\d+|\w+);/gi, (all, name: string) => {
    if (name[0] === "#") {
      const code =
        name[1] === "x" || name[1] === "X"
          ? parseInt(name.slice(2), 16)
          : parseInt(name.slice(1), 10)
      return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : ""
    }
    return ENTITIES[name] ?? all
  })
}
const texts = (part: string) =>
  [...part.matchAll(/<t(?:\s[^>]*)?>([\s\S]*?)<\/t>/g)]
    .map((m) => decode(m[1]!))
    .join("")
const attr = (tag: string, name: string) =>
  new RegExp(`\\s${name}="([^"]*)"`).exec(tag)?.[1]

function readXlsx(bytes: Uint8Array): SheetRow[] {
  const sizeCheck = new Set([
    "xl/workbook.xml",
    "xl/_rels/workbook.xml.rels",
    "xl/sharedStrings.xml",
  ])
  const meta = unzip(bytes, (name) => sizeCheck.has(name))
  const workbook = xml(meta["xl/workbook.xml"])
  const sheet = /<sheet\s[^>]*>/.exec(workbook)?.[0]
  const id = sheet && (attr(sheet, "r:id") ?? attr(sheet, "[\\w]+:id"))
  const rels = xml(meta["xl/_rels/workbook.xml.rels"])
  const rel = [...rels.matchAll(/<Relationship\s[^>]*>/g)]
    .map((m) => m[0])
    .find((tag) => attr(tag, "Id") === id)
  const target = rel && attr(rel, "Target")
  if (!target) throw unreadable()
  const path = target.startsWith("/")
    ? target.slice(1)
    : "xl/" + target.replace(/^\.\//, "")
  if (!/^xl\/worksheets\/[\w.-]+\.xml$/.test(path)) throw unreadable()
  const sheetXml = xml(unzip(bytes, (name) => name === path)[path])
  const shared = [
    ...xml(meta["xl/sharedStrings.xml"], false).matchAll(
      /<si>([\s\S]*?)<\/si>/g
    ),
  ].map((m) => texts(m[1]!))
  const rows: SheetRow[] = []
  for (const row of sheetXml.matchAll(/<row\b([^>]*)>([\s\S]*?)<\/row>/g)) {
    const line = Number(attr(row[1]!, "r") ?? rows.length + 1)
    const cells: string[] = []
    for (const c of row[2]!.matchAll(/<c\b([^>]*?)(?:\/>|>([\s\S]*?)<\/c>)/g)) {
      const ref = attr(c[1]!, "r") ?? ""
      const letters = /^[A-Z]+/.exec(ref)?.[0] ?? ""
      let column = cells.length
      if (letters)
        column =
          [...letters].reduce((n, l) => n * 26 + l.charCodeAt(0) - 64, 0) - 1
      if (column >= MAX_COLUMNS)
        throw new SheetError(
          `Spreadsheets can have at most ${MAX_COLUMNS} columns.`,
          413
        )
      const body = c[2] ?? ""
      const type = attr(c[1]!, "t")
      const value = /<v>([\s\S]*?)<\/v>/.exec(body)?.[1]
      const text =
        type === "s"
          ? (shared[Number(value)] ?? "")
          : type === "inlineStr"
            ? texts(body)
            : decode(value ?? "")
      while (cells.length < column) cells.push("")
      cells[column] = text
    }
    rows.push({ line, cells })
  }
  return rows
}

/** Normalized encoded row (masterplan §14): its JSON must fit 48 KiB, so a chunk of rows fits the API body. */
export function checkRowSize(row: SheetRow) {
  if (
    new TextEncoder().encode(JSON.stringify(row.cells)).length > MAX_ROW_BYTES
  )
    throw new SheetError(`Row ${row.line} is too large to import.`, 413)
}
