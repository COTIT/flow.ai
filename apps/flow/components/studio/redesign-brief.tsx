"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { Button } from "@genesy/ui/components/button"
import { useVoice } from "@genesy/voice/components/provider"
import { briefText, type Brief } from "@/lib/studio"

const AREAS = ["Hero", "Product layout", "Typography", "Colours", "Navigation"]

/**
 * Sends one plain message through the open conversation, or opens a text conversation first
 * (as the dock does). A start that fails sends nothing later.
 */
export function useSendWhenConnected() {
  const voice = useVoice()
  const pending = React.useRef<string | null>(null)
  const asked = React.useRef(false)
  React.useEffect(() => {
    if (voice.connected && pending.current) {
      const text = pending.current
      pending.current = null
      void voice.send(text)
    } else if (asked.current && !voice.starting && !voice.connected) {
      pending.current = null
    }
    asked.current = voice.starting
  }, [voice, voice.connected, voice.starting])
  return (text: string) => {
    if (voice.connected) return void voice.send(text)
    pending.current = text
    void voice.start(true)
  }
}

function Choice({
  pressed,
  onClick,
  children,
}: {
  pressed: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <Button
      variant={pressed ? "default" : "outline"}
      size="sm"
      className="min-h-11 rounded-full"
      aria-pressed={pressed}
      onClick={onClick}
    >
      {children}
    </Button>
  )
}

/** A few quick choices that become one request; the assistant's edit tools and review do the rest. */
export function RedesignBrief({ onPrepare }: { onPrepare: (text: string) => void }) {
  const t = useTranslations("studio")
  const [brief, setBrief] = React.useState<Brief>({
    areas: [],
    keepStructure: true,
    direction: "editorial",
  })
  const toggle = (area: string) =>
    setBrief((b) => ({
      ...b,
      areas: b.areas.includes(area)
        ? b.areas.filter((a) => a !== area)
        : AREAS.filter((a) => a === area || b.areas.includes(a)),
    }))
  return (
    <section
      aria-label={t("briefTitle")}
      className="flex flex-col gap-3 rounded-2xl border bg-card p-4"
    >
      <p className="text-xs font-semibold tracking-widest text-muted-foreground uppercase">
        {t("briefTitle")}
      </p>
      <span className="text-sm font-medium">{t("areasQuestion")}</span>
      <div role="group" aria-label={t("areasLabel")} className="flex flex-wrap gap-2">
        {AREAS.map((area) => (
          <Choice
            key={area}
            pressed={brief.areas.includes(area)}
            onClick={() => toggle(area)}
          >
            {t(`areas.${area}`)}
          </Choice>
        ))}
      </div>
      <span className="text-sm font-medium">{t("keepQuestion")}</span>
      <div role="group" aria-label={t("keepLabel")} className="flex flex-wrap gap-2">
        {([true, false] as const).map((keep) => (
          <Choice
            key={String(keep)}
            pressed={brief.keepStructure === keep}
            onClick={() => setBrief((b) => ({ ...b, keepStructure: keep }))}
          >
            {t(keep ? "keep" : "change")}
          </Choice>
        ))}
      </div>
      <span className="text-sm font-medium">{t("direction")}</span>
      <div role="group" aria-label={t("direction")} className="flex flex-wrap gap-2">
        {(["editorial", "product"] as const).map((direction) => (
          <Choice
            key={direction}
            pressed={brief.direction === direction}
            onClick={() => setBrief((b) => ({ ...b, direction }))}
          >
            {t(direction)}
          </Choice>
        ))}
      </div>
      <Button
        className="min-h-11 self-start"
        disabled={!brief.areas.length}
        onClick={() => onPrepare(briefText(brief))}
      >
        {t("prepare")}
      </Button>
    </section>
  )
}
