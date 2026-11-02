import { describe, expect, it } from "vitest"

import en from "../messages/en.json"
import nl from "../messages/nl.json"
import sw from "../messages/sw.json"

const keys = (o: object, p = ""): string[] =>
  Object.entries(o).flatMap(([k, v]) =>
    typeof v === "object" && v !== null ? keys(v, `${p}${k}.`) : [`${p}${k}`]
  )

describe("landing messages", () => {
  it.each([
    ["nl", nl],
    ["sw", sw],
  ])("%s has exactly the English keys", (_, messages) => {
    expect(keys(messages).sort()).toEqual(keys(en).sort())
  })
})
