"use client"

import { useState } from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { X } from "lucide-react"

import { useRights } from "@genesy/api-client/rights"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { dismissHint, readActivity } from "@/lib/activity"
import { useActivityKey } from "@/lib/pills"

/** Uses after which the hint has done its job. */
const FAMILIAR = 5
const AREAS = [
  ["stock", "stock.list"],
  ["orders", "order.list"],
  ["sales", "order.list"],
  ["invoices", "invoice.list"],
] as const

/**
 * First-visit guidance under the orb: short, dismissible, only about what this role may do.
 * Dismissed or familiar, it shrinks to one quiet "What can Genesy do?" link to the explorer.
 */
export function DiscoveryHint() {
  const t = useTranslations("discovery.hint")
  const locale = useLocale()
  const rights = useRights()
  const key = useActivityKey()
  const [dismissed, setDismissed] = useState(false)
  if (!key || !rights.read) return null
  const { uses, hintDismissed } = readActivity(key)
  const total = Object.values(uses).reduce((sum, use) => sum + use.n, 0)
  // Once the tip is gone, a quiet link keeps the explorer findable (pills may be full).
  if (dismissed || hintDismissed || total >= FAMILIAR)
    return (
      <Link
        href="/explore"
        className={buttonVariants({
          variant: "ghost",
          className: "min-h-11 text-muted-foreground",
        })}
      >
        {t("whatCanDo")}
      </Link>
    )

  const areas = AREAS.filter(([, right]) => rights.may(right))
    .slice(0, 3)
    .map(([area]) => t(`areas.${area}`))
  return (
    <aside
      aria-label={t("lead")}
      className="flex max-w-md animate-in items-start gap-2 rounded-xl border border-(--hair) bg-card/60 p-3 text-left text-sm duration-300 fade-in motion-reduce:animate-none"
    >
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <p>
          <span className="font-semibold">{t("lead")}</span>{" "}
          <span className="text-muted-foreground">
            {areas.length
              ? t("text", {
                  areas: new Intl.ListFormat(locale, {
                    type: "conjunction",
                  }).format(areas),
                })
              : t("textGeneral")}
          </span>
        </p>
        <Link
          href="/explore"
          className={buttonVariants({
            variant: "outline",
            className: "min-h-11 self-start",
          })}
        >
          {t("explore")}
        </Link>
      </div>
      <Button
        variant="ghost"
        size="icon"
        className="min-h-11 min-w-11"
        aria-label={t("dismiss")}
        onClick={() => {
          dismissHint(key)
          setDismissed(true)
        }}
      >
        <X aria-hidden />
      </Button>
    </aside>
  )
}
