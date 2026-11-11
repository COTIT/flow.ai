"use client"

import { useEffect, useRef } from "react"
import { useTranslations } from "next-intl"
import { PanelRight } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { VoiceThinking } from "@genesy/voice/components/activity"
import { VoiceControls } from "@genesy/voice/components/controls"
import { useVoice } from "@genesy/voice/components/provider"
import { TranscriptRows } from "@genesy/voice/components/transcript"
import { flowStore } from "@/lib/flow-store"
import { OrbSlot } from "./orb-layer"
import { OrbCaption } from "./orb-caption"

/** Home while talking: a larger orb, what is said, and the conversation's own controls. */
export function Immersive() {
  const t = useTranslations()
  const voice = useVoice()
  const log = useRef<HTMLDivElement>(null)
  // Keep the newest line in view.
  useEffect(() => {
    const element = log.current
    if (element) element.scrollTop = element.scrollHeight
  }, [voice.messages, voice.thinking])
  return (
    <section
      aria-labelledby="flow-conversation-title"
      className="flex flex-col items-center gap-4 py-4"
    >
      <h1 id="flow-conversation-title" className="sr-only">
        {t("shell.conversation")}
      </h1>
      <OrbSlot name="immersive" className="size-48 sm:size-64" />
      <OrbCaption />
      <VoiceControls />
      <div
        ref={log}
        role="log"
        aria-label={t("voice.transcript")}
        aria-live="polite"
        className="flex max-h-[40svh] min-h-16 w-full min-w-0 flex-col gap-2 overflow-y-auto px-1"
      >
        <TranscriptRows
          rows={voice.messages}
          renderResult={(row) => (
            <Button
              variant="outline"
              data-role="result"
              className="min-h-11 self-start"
              onClick={() =>
                flowStore.setState({ resultOpen: true, resultFocus: row.id })
              }
            >
              <PanelRight aria-hidden />
              {t("flow.chat.showResult")}
            </Button>
          )}
        />
        <VoiceThinking />
      </div>
      <Button
        variant="ghost"
        className="min-h-11"
        onClick={() => flowStore.setState({ immersive: false })}
      >
        {t("shell.backHome")}
      </Button>
    </section>
  )
}
