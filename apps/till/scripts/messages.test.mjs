import assert from "node:assert/strict"
import { readFileSync } from "node:fs"
import { test } from "node:test"

import { createTranslator } from "next-intl"

const load = (locale) =>
  JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url)))

for (const locale of ["en", "nl", "sw"]) {
  const t = createTranslator({ locale, messages: load(locale) })

  test(`${locale}: queue counts agree with their number`, () => {
    for (const key of ["queue.needsOffice"]) {
      const one = t(key, { count: 1 })
      const two = t(key, { count: 2 })
      assert.notEqual(two, one.replace("1", "2"), `${key} has no plural`)
    }
  })

  test(`${locale}: a refused call has a translated title`, () => {
    for (const key of [
      "sale.refused",
      "sale.quoteRefused",
      "drawer.refused",
      "refusals.refReused",
      "refusals.noShift",
      "refusals.alreadyOpen",
    ])
      assert.ok(t.has(key), `${key} missing`)
  })
}
