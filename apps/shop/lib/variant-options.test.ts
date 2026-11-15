import { describe, expect, it } from "vitest"

import { optionTypes, pick } from "./variant-options"

const v = (id: string, ...pairs: [string, string][]) => ({
  id,
  options: pairs.map(([type, value]) => ({ type, value })),
})

// Two colours x two spices, less red + cumin.
const basket = [
  v("1", ["Colour", "Bronze"], ["Spice", "Pepper"]),
  v("2", ["Colour", "Bronze"], ["Spice", "Cumin"]),
  v("3", ["Colour", "Red"], ["Spice", "Pepper"]),
]

describe("optionTypes", () => {
  it("lists each option once, values in the order the variants first use them", () => {
    expect(optionTypes(basket)).toEqual([
      { type: "Colour", values: ["Bronze", "Red"] },
      { type: "Spice", values: ["Pepper", "Cumin"] },
    ])
  })

  it("is empty for a product without options", () => {
    expect(optionTypes([v("1")])).toEqual([])
  })
})

describe("pick", () => {
  it("keeps the other choices when that combination exists", () => {
    expect(pick(basket, basket[0]!, "Spice", "Cumin")?.id).toBe("2")
  })

  it("falls back to the first variant with the new value when the combination is missing", () => {
    expect(pick(basket, basket[1]!, "Colour", "Red")?.id).toBe("3")
  })

  it("starts from nothing chosen", () => {
    expect(pick(basket, null, "Colour", "Red")?.id).toBe("3")
  })

  it("finds nothing for a value no variant has", () => {
    expect(pick(basket, basket[0]!, "Colour", "Blue")).toBeNull()
  })
})
