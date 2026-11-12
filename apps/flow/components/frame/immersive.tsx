"use client"

import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { VoiceThinking } from "@genesy/voice/components/activity"
import { VoiceControls } from "@genesy/voice/components/controls"
import { InlineResult, InlineTask } from "@/components/task/inline-task"
import { useVoice } from "@genesy/voice/components/provider"
import { TranscriptRows } from "@genesy/voice/components/transcript"
import { flowStore } from "@/lib/flow-store"
import { OrbSlot } from "./orb-layer"
import { OrbCaption } from "./orb-caption"

/** Home while talking: the orb (large until there is content, then compact aside), what is said, and the conversation's own controls. */
export function Immersive() {
  const t = useTranslations()
  const voice = useVoice()
  const log = useRef<HTMLDivElement>(null)
  const heading = useRef<HTMLHeadingElement>(null)
  // Opened from the orb: the orb becomes a picture here, so focus moves into the conversation.
  // Likewise when the control that brought us here is gone (a review cancelled, P9-1).
  useEffect(() => {
    const active = document.activeElement
    if (active === document.body || active?.closest("[data-orb]"))
      heading.current?.focus()
  }, [])
  // Switching to voice by keyboard: its button turns disabled (or goes) while upgrading, and the
  // browser would drop focus to the page (P9-4).
  useEffect(() => {
    const active = document.activeElement as HTMLButtonElement | null
    if (voice.upgrading && (active === document.body || active?.disabled))
      heading.current?.focus()
  }, [voice.upgrading])
  // Keep the newest line in view; the page's one scroller scrolls, the log never has its own.
  useEffect(() => {
    const scroller = log.current?.closest("[data-flow-scroll]")
    if (scroller) scroller.scrollTop = scroller.scrollHeight
  }, [voice.messages, voice.thinking])
  // Once there is something to read the orb steps aside (user decision): a compact orb at the
  // start of a header row, and the transcript gets the full height between it and the dock.
  const conversing = voice.messages.length > 0
  const back = (
    <Button
      variant="ghost"
      className="min-h-11 shrink-0"
      onClick={() => flowStore.setState({ immersive: false })}
    >
      {t("shell.backHome")}
    </Button>
  )
  return (
    <section
      aria-labelledby="flow-conversation-title"
      data-conversing={conversing || undefined}
      className={`flex min-h-full flex-col ${conversing ? "gap-3" : "items-center gap-4 py-4"}`}
    >
      <h1
        ref={heading}
        id="flow-conversation-title"
        tabIndex={-1}
        className="sr-only"
      >
        {t("shell.conversation")}
      </h1>
      {conversing ? (
        <div
          data-conversation-header
          className="sticky top-0 z-10 flex flex-wrap items-center gap-x-3 gap-y-1 rounded-2xl glass-strong px-3 py-2"
        >
          <OrbSlot name="compact" className="size-12" />
          {/* The state words keep at least 8rem (ellipsis past that); the controls wrap to the
              next line on narrow bars rather than squeeze them (P10-8). */}
          <div className="min-w-32 flex-[1_1_8rem] text-left [&_*]:min-w-0 [&_*]:truncate [&>*]:justify-start [&>*]:text-left">
            <OrbCaption />
          </div>
          <div className="ml-auto flex shrink-0 flex-wrap items-center gap-2">
            <VoiceControls />
          </div>
          <span className="shrink-0 max-md:hidden">{back}</span>
        </div>
      ) : (
        <>
          <OrbSlot name="immersive" className="size-48 sm:size-64" />
          <OrbCaption />
          <VoiceControls />
        </>
      )}
      <div
        ref={log}
        role="log"
        aria-label={t("voice.transcript")}
        aria-live="polite"
        className="flex min-h-16 w-full min-w-0 flex-1 flex-col gap-2 px-1 [&_p]:max-w-[72ch]"
      >
        <TranscriptRows
          rows={voice.messages}
          renderResult={(row) => <InlineResult row={row} />}
        />
        <VoiceThinking />
      </div>
      {/* The task under the transcript, not inside its live log (read out as one card). */}
      <InlineTask />
      <div className={conversing ? "self-center md:hidden" : "self-center"}>
        {back}
      </div>
    </section>
  )
}
