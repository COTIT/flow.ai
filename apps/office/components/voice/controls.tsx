"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"
import { Mic, MicOff, PhoneOff, VolumeX } from "lucide-react"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { useVoice } from "./provider"
import { VoiceActivity } from "./activity"

export function VoiceControls({ compact = false }: { compact?: boolean }) {
  const voice = useVoice()
  const t = useTranslations("voice")
  if (!voice.connected && !voice.starting && !voice.upgrading)
    return compact ? null : (
      <div className="flex flex-wrap justify-center gap-2">
        <Button className="min-h-11" onClick={() => void voice.start(false)}>
          <Mic aria-hidden />
          {t("start")}
        </Button>
        <Button
          className="min-h-11"
          variant="outline"
          onClick={() => void voice.start(true)}
        >
          {t("textInstead")}
        </Button>
      </div>
    )
  return (
    <div className="flex flex-wrap items-center justify-center gap-2">
      {compact ? <VoiceActivity compact /> : null}
      {compact ? (
        <Link
          href="/voice"
          className={buttonVariants({
            variant: "outline",
            className: "min-h-11",
          })}
        >
          {t("open")}
        </Link>
      ) : null}
      {voice.connected && voice.textOnly ? (
        <Button
          className="min-h-11 min-w-11"
          variant="outline"
          aria-label={t("enableVoice")}
          disabled={voice.busy || Boolean(voice.review)}
          onClick={() => void voice.upgrade()}
        >
          <Mic aria-hidden />
          {!compact ? (
            <span className="sr-only sm:not-sr-only">{t("enableVoice")}</span>
          ) : null}
        </Button>
      ) : null}
      {voice.connected && !voice.textOnly ? (
        <Button
          className="min-h-11 min-w-11"
          variant="outline"
          aria-label={t(
            voice.capturePaused
              ? "resumeVoice"
              : voice.muted
                ? "unmute"
                : "mute"
          )}
          aria-pressed={voice.muted}
          onClick={voice.toggleMute}
        >
          {voice.muted ? <Mic aria-hidden /> : <MicOff aria-hidden />}
          {!compact ? (
            <span className="sr-only sm:not-sr-only">
              {t(
                voice.capturePaused
                  ? "resumeVoice"
                  : voice.muted
                    ? "unmute"
                    : "mute"
              )}
            </span>
          ) : null}
        </Button>
      ) : null}
      {voice.speaking ? (
        <Button
          className="min-h-11 min-w-11"
          variant="outline"
          aria-label={t("stopSpeaking")}
          onClick={voice.stopSpeaking}
        >
          <VolumeX aria-hidden />
          {!compact ? (
            <span className="sr-only sm:not-sr-only">{t("stopSpeaking")}</span>
          ) : null}
        </Button>
      ) : null}
      <Button
        className="min-h-11 min-w-11"
        variant="outline"
        onClick={() => void voice.end().catch(() => undefined)}
        aria-label={t("end")}
      >
        <PhoneOff aria-hidden />
        {!compact ? (
          <span className="sr-only sm:not-sr-only">{t("end")}</span>
        ) : null}
      </Button>
    </div>
  )
}
