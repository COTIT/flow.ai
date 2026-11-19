import { describe, expect, it } from "vitest"

import { parseWake } from "./wake"

describe("typed Hey Verzo", () => {
  it("starts and sends the rest of the instruction", () => {
    expect(parseWake("Hey Verzo, show my pending orders")).toEqual({
      wake: true,
      rest: "show my pending orders",
    })
    expect(parseWake("  hey genesy show sales")).toEqual({
      wake: true,
      rest: "show sales",
    })
    expect(parseWake("Hey, Verzo! what can you do?")).toEqual({
      wake: true,
      rest: "what can you do?",
    })
  })

  it("only starts when nothing follows", () => {
    expect(parseWake("Hey Verzo")).toEqual({ wake: true, rest: "" })
    expect(parseWake("Hey Verzo.")).toEqual({ wake: true, rest: "" })
  })

  it("leaves other text alone", () => {
    expect(parseWake("Hey there, show orders")).toEqual({
      wake: false,
      rest: "Hey there, show orders",
    })
    expect(parseWake("Hey Genesys")).toEqual({
      wake: false,
      rest: "Hey Genesys",
    })
  })
})
