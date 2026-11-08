import { describe, expect, it } from "vitest"
import { strToU8, zipSync } from "fflate"
import { priceRows, readSheet, sheetType, XLSX } from "./sheet"

const csv = (text: string) => strToU8(text)

function xlsx(
  rows: string,
  extra: Record<string, Uint8Array> = {},
  strings = ""
) {
  return zipSync({
    "xl/workbook.xml": strToU8(
      '<workbook xmlns:r="r"><sheets><sheet name="Prices" sheetId="1" r:id="rId7"/><sheet name="Other" sheetId="2" r:id="rId1"/></sheets></workbook>'
    ),
    "xl/_rels/workbook.xml.rels": strToU8(
      '<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId7" Target="/xl/worksheets/sheet2.xml"/></Relationships>'
    ),
    "xl/sharedStrings.xml": strToU8(`<sst>${strings}</sst>`),
    "xl/worksheets/sheet1.xml": strToU8(
      '<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>wrong sheet</t></is></c></row></sheetData></worksheet>'
    ),
    "xl/worksheets/sheet2.xml": strToU8(
      `<worksheet><sheetData>${rows}</sheetData></worksheet>`
    ),
    ...extra,
  })
}

describe("sheetType", () => {
  it("recognises xlsx by zip bytes and csv by text", () => {
    expect(sheetType(xlsx(""), XLSX)).toBe(XLSX)
    expect(sheetType(csv("a,b\n1,2"), "text/csv")).toBe("text/csv")
  })
  it("refuses old .xls and mismatched types in plain words", () => {
    const ole = new Uint8Array([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1])
    expect(() => sheetType(ole, "application/vnd.ms-excel")).toThrow(
      /\.xlsx or \.csv/
    )
    expect(() => sheetType(csv("a,b"), XLSX)).toThrow(/\.xlsx or \.csv/)
    expect(() => sheetType(xlsx(""), "text/csv")).toThrow(/\.xlsx or \.csv/)
  })
})

describe("readSheet csv", () => {
  it("reads quotes, semicolons, BOM and line numbers", () => {
    const sheet = readSheet(
      csv('﻿Name;Price\n"Rice; 5 kg";"12,50"\n\n"Say ""hi""";3\r\n'),
      "text/csv"
    )
    expect(sheet.columns).toEqual(["Name", "Price"])
    expect(sheet.rows).toEqual([
      { line: 2, cells: ["Rice; 5 kg", "12,50"] },
      { line: 4, cells: ['Say "hi"', "3"] },
    ])
  })
  it("refuses text that is not UTF-8", () => {
    expect(() =>
      readSheet(new Uint8Array([0x4e, 0x61, 0xe9, 0x0a]), "text/csv")
    ).toThrow(/UTF-8/)
  })
})

describe("readSheet xlsx", () => {
  it("reads the first sheet with shared and inline strings, numbers kept as written", () => {
    const sheet = readSheet(
      xlsx(
        '<row r="1"><c r="A1" t="s"><v>0</v></c><c r="C1" t="s"><v>1</v></c></row>' +
          '<row r="3"><c r="A3" t="inlineStr"><is><t>Beans &amp; peas</t></is></c><c r="B3"/><c r="C3"><v>0.1</v></c></row>' +
          '<row r="4"><c r="A4" t="s"><v>2</v></c><c r="C4"><v>12.5</v></c></row>',
        {},
        '<si><t>Product</t></si><si><r><t>Pri</t></r><r><t xml:space="preserve">ce</t></r></si><si><t>Rice &#x31;kg</t></si>'
      ),
      XLSX
    )
    expect(sheet.columns).toEqual(["Product", "", "Price"])
    expect(sheet.rows).toEqual([
      { line: 3, cells: ["Beans & peas", "", "0.1"] },
      { line: 4, cells: ["Rice 1kg", "", "12.5"] },
    ])
  })
  it("refuses a zip bomb before inflating it", () => {
    const bomb = xlsx("", {
      "xl/worksheets/sheet2.xml": new Uint8Array(21 * 1024 * 1024),
    })
    expect(bomb.length).toBeLessThan(2 * 1024 * 1024)
    expect(() => readSheet(bomb, XLSX)).toThrow(/too large/)
  })
  it("refuses more than 50 zip entries", () => {
    const extra: Record<string, Uint8Array> = {}
    for (let i = 0; i < 50; i++) extra[`junk/${i}.txt`] = strToU8("x")
    expect(() => readSheet(xlsx("", extra), XLSX)).toThrow(/too many parts/)
  })
  it("refuses XML with a DTD or entity", () => {
    const dtd = xlsx("", {
      "xl/sharedStrings.xml": strToU8(
        '<!DOCTYPE x [<!ENTITY a "aaaa">]><sst><si><t>&a;</t></si></sst>'
      ),
    })
    expect(() => readSheet(dtd, XLSX)).toThrow(/cannot be read/)
  })
  it("refuses a corrupt workbook in plain words", () => {
    expect(() => readSheet(strToU8("PK\u0003\u0004broken"), XLSX)).toThrow(
      /cannot be read/
    )
  })
})

describe("limits", () => {
  const rows = (n: number) =>
    "Name,Price\n" +
    Array.from({ length: n }, (_, i) => `P${i},${i}`).join("\n")
  it("accepts 500 rows and refuses 501", () => {
    expect(readSheet(csv(rows(500)), "text/csv").rows).toHaveLength(500)
    expect(() => readSheet(csv(rows(501)), "text/csv")).toThrow(/500 rows/)
  })
  it("refuses more than 30 columns and over-long cells", () => {
    const wide = Array.from({ length: 31 }, (_, i) => `c${i}`).join(",")
    expect(() => readSheet(csv(wide + "\n" + wide), "text/csv")).toThrow(
      /30 columns/
    )
    expect(() =>
      readSheet(csv("Name,Price\n" + "x".repeat(256) + ",1"), "text/csv")
    ).toThrow(/255 characters/)
  })
  it("refuses an empty sheet", () => {
    expect(() => readSheet(csv("\n\n"), "text/csv")).toThrow(/no rows/)
  })
})

describe("priceRows", () => {
  const sheet = readSheet(
    csv(
      "Product,Unit price,Notes\nRice,12.50,x\n,,\nBeans,,y\nMaize,3,z\nOil,9,\n"
    ),
    "text/csv"
  )
  it("finds the product and price columns from their headers", () => {
    const page = priceRows(sheet, {}, 2)
    expect(page).toEqual({
      columns: { product: "Product", price: "Unit price" },
      from: 1,
      to: 2,
      total: 4,
      next: 3,
      rows: [
        { line: 2, key: "Rice", price: "12.50" },
        { line: 4, key: "Beans", price: "" },
      ],
    })
    expect(priceRows(sheet, { fromRow: 3 }, 2)).toMatchObject({
      from: 3,
      to: 4,
      next: undefined,
      rows: [{ key: "Maize" }, { key: "Oil" }],
    })
  })
  it("uses named columns and asks when headers are unclear", () => {
    const unclear = readSheet(csv("A,B\nRice,1\n"), "text/csv")
    expect(priceRows(unclear, {})).toEqual({ needsColumns: ["A", "B"] })
    expect(
      priceRows(unclear, { productColumn: "a", priceColumn: "B" })
    ).toMatchObject({ rows: [{ line: 2, key: "Rice", price: "1" }] })
    const two = readSheet(csv("Name,SKU,Price\nRice,R1,1\n"), "text/csv")
    expect(priceRows(two, {})).toEqual({
      needsColumns: ["Name", "SKU", "Price"],
    })
    expect(() =>
      priceRows(unclear, { productColumn: "Nope", priceColumn: "B" })
    ).toThrow(/column/)
  })
  it("refuses a start row past the end", () => {
    expect(() => priceRows(sheet, { fromRow: 9 })).toThrow(/row/)
  })
})
