"use client"

import { RotateCcw } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@genesy/ui/components/button"
import { useVoice } from "./provider"

/** One retry after a request ended without an answer; the chat line above says why. */
export function VoiceUnfinished() {
  const voice = useVoice()
  const t = useTranslations("voice")
  if (!voice.unfinished || voice.starting) return null
  return (
    <Button
      variant="outline"
      data-voice-unfinished={voice.unfinished}
      className="min-h-11 self-start"
      onClick={() => void voice.resume()}
    >
      <RotateCcw aria-hidden className="size-4" />
      {t(
        voice.unfinished === "dropped"
          ? "unfinished.continue"
          : "unfinished.askAgain"
      )}
    </Button>
  )
}
