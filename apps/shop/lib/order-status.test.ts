import { createTranslator } from "next-intl"
import { describe, expect, it } from "vitest"

import en from "../messages/en.json"
import nl from "../messages/nl.json"
import sw from "../messages/sw.json"

// Every status the API can give an order (orders/dto.rs STATUSES, plus "refunded"), and one it
// may add later: the order page says it in words, never as the code.
const statuses = [
  "draft",
  "awaiting_payment",
  "open",
  "completed",
  "cancelled",
  "partially_returned",
  "returned",
  "refunded",
  "some_new_status",
]

describe.each([
  ["en", en],
  ["nl", nl],
  ["sw", sw],
])("%s order status", (locale, messages) => {
  const t = createTranslator({ locale, messages, namespace: "order" })
  it.each(statuses)("%s is a word", (status) => {
    const text = t("status", { status })
    expect(text).not.toContain(status)
    expect(text).not.toMatch(/[a-z]+_[a-z]+/)
  })
})
