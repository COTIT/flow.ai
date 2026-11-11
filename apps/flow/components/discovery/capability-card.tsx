"use client"

import { useTranslations } from "next-intl"

import { Badge } from "@genesy/ui/components/badge"
import { Button } from "@genesy/ui/components/button"
import type { FlowCapability, FlowStatus } from "@genesy/voice/lib/capabilities"

const BADGE = {
  available: "secondary",
  approval: "outline",
  restricted: "outline",
  planned: "outline",
} as const

/** What it does, one example, its status in words, and a Try it only when it can be used. */
export function CapabilityCard({
  cap,
  status,
  onTry,
}: {
  cap: FlowCapability
  status: FlowStatus
  onTry: () => void
}) {
  const t = useTranslations("discovery")
  const name = t(`capabilities.${cap.id}.label`)
  const usable = status === "available" || status === "approval"
  return (
    <li
      data-capability={cap.id}
      data-status={status}
      className="flex min-w-0 flex-col gap-2 rounded-2xl border border-(--hair) bg-card/60 p-4 text-left"
    >
      <div className="flex flex-wrap items-center gap-2">
        <h3 className="mr-auto font-semibold">{name}</h3>
        <Badge variant={BADGE[status]}>{t(`status.${status}`)}</Badge>
      </div>
      <p className="text-sm text-muted-foreground">
        {t(`capabilities.${cap.id}.description`)}
      </p>
      <p className="text-sm">
        {t("explorer.example", {
          example: t(`capabilities.${cap.id}.example`),
        })}
      </p>
      {status !== "available" ? (
        <p className="text-xs text-muted-foreground">
          {t(`statusText.${status}`)}
        </p>
      ) : null}
      {usable ? (
        <Button
          variant="outline"
          className="min-h-11 self-start"
          aria-label={t("explorer.tryName", { name })}
          onClick={onTry}
        >
          {t("explorer.try")}
        </Button>
      ) : null}
    </li>
  )
}
