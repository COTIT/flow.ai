"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"

import { useVoice } from "@genesy/voice/components/provider"
import type { TranscriptRow } from "@genesy/voice/lib/helpers"
import { flowStore, useFlow } from "@/lib/flow-store"
import { useTask } from "@/lib/task-store"
import { BatchReview } from "./batch-review"
import { Clarify } from "./clarify"
import { Expired } from "./expired"
import { Receipt } from "./receipt"
import { ResultsList, ReviewPanel } from "./results"
import { Withdrawn } from "./withdrawn"

/**
 * The task scene; the orb stays compact. A choice Genesy asked for, the action to approve (a
 * replaced proposal shown as never sent), then its receipt in the same place, then the results.
 */
export function ApprovalScene() {
  const t = useTranslations("flow.chat")
  const voice = useVoice()
  const focus = useFlow((s) => s.resultFocus)
  const decided = useTask((s) => s.decided)
  const result = useTask((s) => s.result)
  const withdrawn = useTask((s) => s.withdrawn)
  const results = voice.messages.filter(
    (row): row is TranscriptRow => row.role === "result" && !!row.views
  )
  // Opened by its address with nothing left to show (e.g. a reload ended the conversation):
  // home instead of an empty scene; replaced, so Back never returns to it.
  const live = voice.connected || voice.starting || voice.upgrading
  const empty = !live && !voice.review && !decided && !withdrawn && !results.length
  useEffect(() => {
    if (!empty) return
    history.replaceState(history.state, "", location.pathname)
    flowStore.setState({ resultOpen: false })
  }, [empty])
  useEffect(() => {
    if (!focus) return
    const element = document.getElementById(`result-${focus}`)
    element?.scrollIntoView({ block: "nearest" })
    element?.focus({ preventScroll: true })
    flowStore.setState({ resultFocus: null })
  }, [focus])
  return (
    <section aria-labelledby="flow-results-title" className="flex flex-col gap-3">
      <h1 id="flow-results-title" className="text-base font-semibold">
        {t("results")}
      </h1>
      <Withdrawn />
      <Expired />
      <Clarify />
      <BatchReview />
      <ReviewPanel withButtons />
      {decided && result && !voice.review ? (
        <Receipt review={decided} outcome={result} />
      ) : null}
      <ResultsList rows={results} />
    </section>
  )
}
