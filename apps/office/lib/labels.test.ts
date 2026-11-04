import { describe, expect, it } from "vitest"

import { AppKeyKind, AppKeyScope, WebhookEvent } from "@genesy/api-client/model"
import en from "../messages/en.json"
import nl from "../messages/nl.json"
import sw from "../messages/sw.json"
import {
  INVOICE_STATUSES,
  LOT_EVENT_KINDS,
  MOVEMENT_TYPES,
  PAY_METHODS,
  labelKey,
} from "./labels"

// Every code the API can send has a word in every language, so no screen falls back to the code.
const maps: [string, readonly string[]][] = [
  ["appKeys.kinds", Object.values(AppKeyKind)],
  ["appKeys.scopeNames", Object.values(AppKeyScope)],
  ["appKeys.scopeHints", Object.values(AppKeyScope)],
  ["webhooks.eventNames", Object.values(WebhookEvent)],
  ["invoices.statuses", INVOICE_STATUSES],
  ["payMethods", PAY_METHODS],
  ["stock.movementTypes", MOVEMENT_TYPES],
  ["lots.eventKinds", LOT_EVENT_KINDS],
]
const at = (m: unknown, path: string) =>
  path.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown>)?.[k], m)

describe.each([
  ["en", en],
  ["nl", nl],
  ["sw", sw],
])("%s labels", (_, messages) => {
  it.each(maps)("%s has a word for every code", (path, codes) => {
    const missing = codes.filter(
      (c) => typeof at(messages, `${path}.${labelKey(c)}`) !== "string"
    )
    expect(missing).toEqual([])
  })
})

describe("labelKey", () => {
  it("turns scope keys and event names into message keys", () => {
    expect(labelKey("catalog:read")).toBe("catalog_read")
    expect(labelKey("stock.changed")).toBe("stock_changed")
    expect(labelKey("partially_paid")).toBe("partially_paid")
  })
})
