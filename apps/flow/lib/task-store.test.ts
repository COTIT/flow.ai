import { expect, it } from "vitest"

import type { VoiceReview } from "@genesy/voice/lib/client-contract"
import { reviewLeft } from "./task-store"

const review = (ticket: string) => ({ ticket }) as VoiceReview
const earlier = { status: "completed", data: [] }

it("a review still open, or the same ticket, changes nothing", () => {
  expect(reviewLeft(null, review("a"), null, null)).toBeNull()
  expect(reviewLeft(review("a"), review("a"), earlier, earlier)).toBeNull()
})

it("a cancelled review is withdrawn: never sent", () => {
  expect(
    reviewLeft(review("a"), null, { status: "cancelled" }, earlier)
  ).toEqual({ withdrawn: review("a"), decided: null, result: null })
})

it("an answered review is decided; its outcome becomes the receipt", () => {
  for (const status of ["completed", "failed", "unknown", "partial"])
    expect(reviewLeft(review("a"), null, { status }, earlier)).toEqual({
      decided: review("a"),
      withdrawn: null,
      // Its own answer is kept: later answers (another question) never become this receipt.
      result: { status },
    })
})

it("a review left with no answer of its own is neither decided nor withdrawn", () => {
  // The connection dropped: the outcome on screen is an earlier read's, not this review's.
  expect(reviewLeft(review("a"), null, earlier, earlier)).toBeNull()
  expect(reviewLeft(review("a"), null, null, null)).toBeNull()
})
