import { describe, expect, it } from "vitest"
import { strToU8 } from "fflate"
import { checkRowSize, MAX_ROW_BYTES, readSheet, SheetError } from "./sheet"

describe("shared import parser", () => {
  it("reads csv into columns and rows", () => {
    const sheet = readSheet(strToU8("name,price\nRice,2.50\n"), "text/csv")
    expect(sheet.columns).toEqual(["name", "price"])
    expect(sheet.rows).toEqual([{ line: 2, cells: ["Rice", "2.50"] }])
  })

  it("refuses with SheetError and a status, not a voice error", () => {
    const rows = ["name", ...Array.from({ length: 501 }, (_, i) => `r${i}`)]
    const read = () => readSheet(strToU8(rows.join("\n")), "text/csv")
    expect(read).toThrow(SheetError)
    expect(read).toThrow(/at most 500 rows/)
    try {
      read()
    } catch (error) {
      expect((error as SheetError).status).toBe(413)
    }
  })

  it("refuses a row whose normalized encoding exceeds 48 KiB", () => {
    const fat = { line: 7, cells: ["x".repeat(MAX_ROW_BYTES)] }
    expect(() => checkRowSize(fat)).toThrow("Row 7 is too large to import.")
    expect(() =>
      checkRowSize({ line: 8, cells: ["x".repeat(MAX_ROW_BYTES - 10)] })
    ).not.toThrow()
  })
})
