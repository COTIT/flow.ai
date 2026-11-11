import { expect, it } from "vitest"

import type { VoiceReview } from "@genesy/voice/lib/client-contract"
import { ownerKey, reviewLeft, taskFor } from "./task-store"

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

it("task state belongs to one workspace and login; anything else sees an empty task", () => {
  const state = {
    owner: ownerKey("w1", "token-a"),
    decided: review("a"),
    result: { status: "completed" },
    withdrawn: review("b"),
    resumeDismissed: true,
  }
  expect(taskFor(state, ownerKey("w1", "token-a"))).toBe(state)
  for (const other of [ownerKey("w2", "token-a"), ownerKey("w1", "token-b"), ownerKey(null, null)])
    expect(taskFor(state, other)).toMatchObject({
      decided: null,
      result: null,
      withdrawn: null,
      resumeDismissed: false,
    })
  // No login or no workspace is nobody's: never matches a stored owner, even another empty one.
  expect(ownerKey(null, "token-a")).toBeNull()
  expect(ownerKey("w1", null)).toBeNull()
  expect(taskFor({ ...state, owner: null }, null).decided).toBeNull()
})
