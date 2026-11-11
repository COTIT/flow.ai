import { expect, it } from "vitest"

import type { VoiceReview } from "@genesy/voice/lib/client-contract"

import { stockLevels } from "./proposed"

const review = (details: Record<string, unknown>) => ({ details }) as VoiceReview

it("lists each proposed total with its warehouse name from the review's snapshot", () => {
  const r = review({
    changes: { warehouses: [{ warehouse_id: "wh-main", stock_lvl: 120 }, { warehouse_id: "wh-2", stock_lvl: "5" }] },
    current: { locked: { slots: [{ warehouse_id: "wh-main", warehouse_name: "Main" }] } },
  })
  expect(stockLevels(r)).toEqual([
    { warehouse: "wh-main", name: "Main", total: "120" },
    { warehouse: "wh-2", name: "", total: "5" },
  ])
})

it("a review without stock levels (or a malformed one) proposes nothing", () => {
  expect(stockLevels(review({}))).toEqual([])
  expect(stockLevels(review({ changes: { warehouses: "x" } }))).toEqual([])
  expect(stockLevels(review({ changes: { warehouses: [{ warehouse_id: "w" }] } }))).toEqual([])
})
