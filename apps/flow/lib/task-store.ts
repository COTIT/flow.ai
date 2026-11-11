import { useEffect, useRef } from "react"
import { useStore } from "zustand"
import { createStore } from "zustand/vanilla"

import type { VoiceOutcome, VoiceReview } from "@genesy/voice/lib/client-contract"

type TaskState = {
  /** The review the person last decided; with the provider's outcome it is the receipt. */
  decided: VoiceReview | null
  /** The decided review's own answer, kept: later answers never become its receipt. */
  result: VoiceOutcome | null
  /** A proposal cancelled before it was sent (a correction or Cancel): shown as never sent. */
  withdrawn: VoiceReview | null
  /** The resume prompt was dismissed for this ended conversation. */
  resumeDismissed: boolean
}

/** Business-task memory of the shell; the voice provider stays the only source of live state. */
export const taskStore = createStore<TaskState>()(() => ({
  decided: null,
  result: null,
  withdrawn: null,
  resumeDismissed: false,
}))
export const useTask = <T,>(pick: (state: TaskState) => T) =>
  useStore(taskStore, pick)

/** What a review leaving means: withdrawn (cancelled, never sent), decided (answered), or nothing. */
export function reviewLeft(
  before: VoiceReview | null,
  review: VoiceReview | null,
  outcome: VoiceOutcome | null,
  earlier: VoiceOutcome | null
): Pick<TaskState, "decided" | "withdrawn" | "result"> | null {
  if (!before || before.ticket === review?.ticket) return null
  // Left with no answer of its own (e.g. the connection dropped): neither sent nor decided here.
  if (!outcome || outcome === earlier) return null
  if (outcome.status === "cancelled")
    return { withdrawn: before, decided: null, result: null }
  return review ? null : { decided: before, withdrawn: null, result: outcome }
}

/**
 * Watches the provider once (from the shell): when a review leaves, it was either cancelled
 * (withdrawn, never sent) or decided (its outcome is the receipt). A new conversation starts clean.
 */
export function useTaskWatch(voice: {
  review: VoiceReview | null
  outcome: VoiceOutcome | null
  starting: boolean
}) {
  const last = useRef<VoiceReview | null>(null)
  // The outcome showing while the review was open belongs to earlier work, never to this review.
  const earlier = useRef<VoiceOutcome | null>(null)
  const { review, outcome, starting } = voice
  useEffect(() => {
    const before = last.current
    last.current = review
    if (review && before?.ticket !== review.ticket) earlier.current = outcome
    const next = reviewLeft(before, review, outcome, earlier.current)
    if (next) taskStore.setState(next)
  }, [review, outcome])
  useEffect(() => {
    if (starting) taskStore.setState(taskStore.getInitialState(), true)
  }, [starting])
}
