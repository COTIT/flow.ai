"use client"

import { useEffect } from "react"
import { useTranslations } from "next-intl"

import { useVoice } from "@genesy/voice/components/provider"
import type { TranscriptRow } from "@genesy/voice/lib/helpers"
import { ResultsList, ReviewPanel } from "@/components/conversation/results"
import { flowStore, useFlow } from "@/lib/flow-store"

/** What the assistant found and what waits for approval, as a full scene; the orb stays compact. */
export function ResultScene() {
  const t = useTranslations("flow.chat")
  const voice = useVoice()
  const focus = useFlow((s) => s.resultFocus)
  const results = voice.messages.filter(
    (row): row is TranscriptRow => row.role === "result" && !!row.views
  )
  useEffect(() => {
    if (!focus) return
    const element = document.getElementById(`result-${focus}`)
    element?.scrollIntoView({ block: "nearest" })
    element?.focus({ preventScroll: true })
    flowStore.setState({ resultFocus: null })
  }, [focus])
  return (
    <section
      aria-labelledby="flow-results-title"
      className="flex flex-col gap-3"
    >
      <h1 id="flow-results-title" className="text-base font-semibold">
        {t("results")}
      </h1>
      <ReviewPanel withButtons />
      <ResultsList rows={results} />
    </section>
  )
}
