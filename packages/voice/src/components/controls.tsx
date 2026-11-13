"use client"

import Link from "next/link"
import { useEffect, useState } from "react"
import { useTranslations } from "next-intl"
import { AudioLines, Mic, MicOff, PhoneOff, VolumeX } from "lucide-react"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { useVoice } from "./provider"
import { VoiceActivity } from "./activity"

export function VoiceControls({
  compact = false,
  openHref = "/voice",
}: {
  compact?: boolean
  /** The host's assistant page, linked from the compact controls. */
  openHref?: string
}) {
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
          href={openHref}
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
          aria-label={t(voice.micMode === "hold" ? "openMic" : "pushToTalk")}
          onClick={() =>
            voice.setMicMode(voice.micMode === "hold" ? "open" : "hold")
          }
        >
          {voice.micMode === "hold" ? (
            <AudioLines aria-hidden />
          ) : (
            <Mic aria-hidden />
          )}
          {!compact ? (
            <span className="sr-only sm:not-sr-only">
              {t(voice.micMode === "hold" ? "openMic" : "pushToTalk")}
            </span>
          ) : null}
        </Button>
      ) : null}
      {compact ? <HoldToTalk compact /> : null}
      {voice.connected &&
      !voice.textOnly &&
      (voice.micMode === "open" || voice.capturePaused) ? (
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
        className={`min-h-11 min-w-11 ${compact ? "" : "border-destructive/40 text-destructive"}`}
        variant="outline"
        onClick={() => void voice.end().catch(() => undefined)}
        aria-label={t(voice.historyEnabled ? "endCall" : "end")}
      >
        <PhoneOff aria-hidden />
        {/* P6: a word on every size; the short one where the full one doesn't fit. */}
        {!compact ? (
          <>
            <span className="sm:hidden">
              {t(voice.historyEnabled ? "endCall" : "endShort")}
            </span>
            <span className="max-sm:hidden">
              {t(voice.historyEnabled ? "endCall" : "end")}
            </span>
          </>
        ) : null}
      </Button>
    </div>
  )
}

const editable = (target: EventTarget | null) =>
  target instanceof HTMLElement &&
  (target.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT", "BUTTON", "A"].includes(target.tagName))

/** Push-to-talk: the mic is open only while this is held (pointer, touch, or Space). */
export function HoldToTalk({ compact = false }: { compact?: boolean }) {
  const voice = useVoice()
  const t = useTranslations("voice")
  const [held, setHeld] = useState(false)
  const shown =
    voice.connected &&
    !voice.textOnly &&
    voice.micMode === "hold" &&
    !voice.capturePaused
  const press = (on: boolean) => {
    setHeld(on)
    voice.talk(on)
  }
  useEffect(() => {
    if (!shown || compact) return
    const down = (event: KeyboardEvent) => {
      if (event.code !== "Space" || event.repeat || editable(event.target))
        return
      event.preventDefault()
      setHeld(true)
      voice.talk(true)
    }
    const up = (event: KeyboardEvent) => {
      if (event.code !== "Space" || editable(event.target)) return
      setHeld(false)
      voice.talk(false)
    }
    const blur = () => {
      setHeld(false)
      voice.talk(false)
    }
    window.addEventListener("keydown", down)
    window.addEventListener("keyup", up)
    window.addEventListener("blur", blur)
    return () => {
      window.removeEventListener("keydown", down)
      window.removeEventListener("keyup", up)
      window.removeEventListener("blur", blur)
    }
  }, [shown, compact, voice])
  if (!shown) return null
  return (
    <Button
      type="button"
      variant={held ? "default" : "outline"}
      aria-pressed={held}
      aria-label={t("holdToTalk")}
      data-voice-hold=""
      className={
        compact
          ? "min-h-11 min-w-11 touch-none transition-transform duration-100 select-none active:scale-[0.97] motion-reduce:transition-none"
          : "min-h-14 w-full max-w-sm touch-none self-center transition-transform duration-100 select-none active:scale-[0.97] motion-reduce:transition-none sm:w-auto sm:px-8"
      }
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId)
        press(true)
      }}
      onPointerUp={() => press(false)}
      onPointerCancel={() => press(false)}
      onLostPointerCapture={() => held && press(false)}
      onKeyDown={(event) => {
        if ((event.key === " " || event.key === "Enter") && !event.repeat) {
          event.preventDefault()
          press(true)
        }
      }}
      onKeyUp={(event) => {
        if (event.key === " " || event.key === "Enter") press(false)
      }}
      onContextMenu={(event) => event.preventDefault()}
    >
      <Mic aria-hidden />
      {compact ? null : t(held ? "talking" : "holdToTalk")}
    </Button>
  )
}
