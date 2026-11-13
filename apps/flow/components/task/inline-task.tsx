"use client"

import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"
import { usePathname, useRouter } from "next/navigation"

import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { VoiceResult } from "@genesy/voice/components/results/index"
import type { TranscriptRow } from "@genesy/voice/lib/helpers"
import { flowStore, useFlow } from "@/lib/flow-store"
import { useTask } from "@/lib/task-store"
import { BatchReview } from "./batch-review"
import { Clarify } from "./clarify"
import { Expired } from "./expired"
import { Receipt } from "./receipt"
import { ReviewPanel } from "./results"
import { Withdrawn } from "./withdrawn"

/**
 * The task where the conversation is (P5): pinned under the transcript, just above the message box,
 * in place of the Results screen. The same parts that screen showed (a choice, a batch, an expired
 * or withdrawn notice, the review with "Yes, do it" / "No, cancel"), then the receipt in the same
 * place. Details stay folded; on short screens it is compact so the buttons stay in reach.
 */
export function InlineTask() {
  const voice = useVoice()
  const decided = useTask((s) => s.decided)
  const result = useTask((s) => s.result)
  const pathname = usePathname()
  // Home's task screen shows it already (no live conversation, or "Show all"): never twice.
  const elsewhere = useFlow((s) => s.resultOpen) && pathname === "/"
  const box = useRef<HTMLDivElement>(null)
  // On screen when any part rendered: the capsule would repeat it (it stays the way back).
  useEffect(() => {
    const on = Boolean(box.current?.childElementCount)
    if (flowStore.getState().taskShown === on) return
    flowStore.setState({ taskShown: on })
    // The card went (a review cancelled) with the focused button: focus moves to the conversation
    // it leaves, never to the page (P9-1).
    if (!on && document.activeElement === document.body) {
      const pane = box.current?.closest("[aria-labelledby]")
      const id = pane?.getAttribute("aria-labelledby")
      if (id) document.getElementById(id)?.focus({ preventScroll: true })
    }
  })
  useEffect(() => () => flowStore.setState({ taskShown: false }), [])
  // The capsule's button asked for it: the card takes focus once on screen.
  const asked = useFlow((s) => s.taskAsked)
  const answered = useRef(asked)
  useEffect(() => {
    if (asked === answered.current) return
    answered.current = asked
    requestAnimationFrame(() => {
      const target = box.current?.querySelector<HTMLElement>(
        "h3[tabindex], [tabindex='-1'], button"
      )
      target?.focus({ preventScroll: true })
      target?.scrollIntoView({ block: "nearest" })
    })
  }, [asked])
  return (
    <div
      ref={box}
      data-inline-task=""
      className="flex shrink-0 flex-col gap-2 empty:hidden [&_#flow-review-title]:scroll-my-4 [&_#flow-review-title]:text-base [&_[data-review-buttons]>button]:min-h-12 max-sm:[&_[data-review-buttons]]:flex-col-reverse max-sm:[&_[data-review-buttons]>button]:w-full [&_[data-review-diff]]:border-0 [&_[data-review-diff]]:bg-transparent [&_[data-review-diff]]:p-0 [@media(max-height:500px)]:[&_#flow-review-title]:sr-only [@media(max-height:500px)]:[&_[data-review-buttons]>button]:min-h-11 [@media(max-height:500px)]:[&_button[aria-controls]]:hidden [@media(max-height:500px)]:[&>section]:gap-1.5 [@media(max-height:500px)]:[&>section]:p-2"
    >
      {elsewhere ? null : (
        <>
          <Withdrawn />
          <Expired />
          <Clarify />
          <BatchReview />
          <ReviewPanel withButtons />
          {decided && result && !voice.review ? (
            <Receipt review={decided} outcome={result} />
          ) : null}
        </>
      )}
    </div>
  )
}

/** Complete result under the reply; large results can also open in their own screen. */
export function InlineResult({ row }: { row: TranscriptRow }) {
  const t = useTranslations("scenes")
  const pathname = usePathname()
  const router = useRouter()
  const views = row.views ?? []
  const first = views[0]
  const rows =
    first && "rows" in first && Array.isArray(first.rows)
      ? first.rows.length
      : 0
  const more = views.length > 1 || rows > 3
  return (
    <div
      data-inline-result=""
      className="flex w-full min-w-0 flex-col gap-1 self-start [&_table]:text-sm"
    >
      <VoiceResult views={views} />
      {more ? (
        <Button
          variant="link"
          className="min-h-11 self-start px-1"
          onClick={() => {
            flowStore.setState({ resultOpen: true, resultFocus: row.id })
            // The Results screen is an address: without it the shell closes it again (J1).
            if (pathname !== "/") router.push("/?view=task")
          }}
        >
          {t("showAll")}
        </Button>
      ) : null}
    </div>
  )
}
