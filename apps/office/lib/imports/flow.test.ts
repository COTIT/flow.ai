import { describe, expect, it, vi } from "vitest"
import { autoMapping, chunkRows, FIELDS, toRows } from "./flow"

const sheet = {
  columns: ["Name", "Price", "Barcode", "Colour"],
  rows: [
    { line: 2, cells: ["Rice", "2.50", "abc", "white"] },
    { line: 3, cells: ["", "1", "", ""] },
  ],
}

describe("import flow helpers", () => {
  it("maps headers that name a field exactly (case and spaces ignored)", () => {
    expect(
      autoMapping(["Company name", "E-mail", "City"], "relations")
    ).toEqual({
      company_name: 0,
      city: 2,
    })
    expect(autoMapping(sheet.columns, "products")).toEqual({
      name: 0,
      price: 1,
      barcode: 2,
    })
  })

  it("builds rows from mapped columns only, numbering data rows and skipping empty cells", () => {
    const rows = toRows(sheet, { name: 0, price: 1 }, 2)
    expect(rows).toEqual([
      {
        row_number: 1,
        external_key: "abc",
        fields: { name: "Rice", price: "2.50" },
      },
      { row_number: 2, fields: { price: "1" } },
    ])
  })

  it("chunks rows by 20", () => {
    const rows = Array.from({ length: 45 }, (_, i) => ({
      row_number: i + 1,
      fields: {},
    }))
    expect(chunkRows(rows).map((c) => c.length)).toEqual([20, 20, 5])
  })

  it("offers only the columns the API accepts per profile", () => {
    expect(FIELDS.products).toContain("barcode")
    expect(FIELDS.products).not.toContain("purchase_price")
    expect(FIELDS.relations).toContain("company_name")
    vi.fn()
  })
})
