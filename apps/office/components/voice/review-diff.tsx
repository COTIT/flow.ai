"use client"

import { useId, useState, type ReactNode } from "react"
import { useLocale, useTranslations } from "next-intl"
import { ArrowRight, ChevronDown, TrendingDown, TrendingUp } from "lucide-react"
import { Notice } from "@genesy/ui/components/notice"
import { formatShare } from "@genesy/ui/lib/format"
import type { VoiceReview } from "@/lib/voice/client-contract"
import { fieldName } from "./helpers"
import { reviewSummary } from "./review-changes"

/** The review card leads with what the API says matters: the record, warnings, impact counts and
 * changed fields (old struck through → new). Everything else folds under "More details".
 * Without any of these the card stays as it was: old values are never guessed. */
export function ReviewDiff({ review, children }: { review: VoiceReview; children: ReactNode }) {
  const v = useTranslations("voice")
  const t = useTranslations("voice.results")
  const labels = useTranslations("voice.reviewFields")
  const locale = useLocale()
  const [open, setOpen] = useState(false)
  const moreId = useId()
  const summary = reviewSummary(review.details)
  if (!summary) return children
  const large = summary.rows.some((row) => row.warning)
  const type = summary.record?.type
  return (
    <div className="flex flex-col gap-3">
      {summary.warnings.length || large ? (
        <Notice variant="warning" title={t("warningsTitle")}>
          <ul className="flex flex-col gap-1">
            {summary.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
            {large ? <li>{t("largeChange")}</li> : null}
          </ul>
        </Notice>
      ) : null}
      <section aria-label={t("diffTitle")} data-review-diff="" className="flex flex-col gap-3 rounded-2xl border border-(--gold) bg-card p-4">
        {summary.record ? (
          <p className="text-base font-semibold">
            {t("recordLine", {
              type: type && v.has(`resources.${type}`) ? v(`resources.${type}`) : (type ?? "").replaceAll("_", " "),
              label: summary.record.label ?? t("nameUnavailable"),
            })}
          </p>
        ) : null}
        {summary.effect && t.has(`effect.${summary.effect}`) ? (
          <p className="text-sm">{t(`effect.${summary.effect}`)}</p>
        ) : null}
        {summary.impact.length ? (
          <div className="flex flex-col gap-1">
            <h3 className="text-xs text-muted-foreground">{t("impactTitle")}</h3>
            <ul className="flex flex-wrap gap-2">
              {summary.impact.map(({ key, count }) => (
                <li key={key} className="rounded-full border border-(--hair) px-3 py-1 text-xs font-medium tabular-nums">
                  {t.has(`impact.${key}`) ? t(`impact.${key}`) : fieldName(key)} {count}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
        {summary.rows.length ? (
          <div className="flex flex-col gap-2">
            <h3 className="text-xs text-muted-foreground">{t("diffTitle")}</h3>
            <dl className="flex flex-col gap-3">
              {summary.rows.map((row) => (
                <div key={row.field} className="flex min-w-0 flex-col gap-1">
                  <dt className="text-sm font-medium">
                    {row.label ?? (labels.has(row.field) ? labels(row.field) : fieldName(row.field))}
                  </dt>
                  <dd className="flex flex-wrap items-center gap-x-2 gap-y-1 text-base break-all tabular-nums">
                    {row.before !== null ? (
                      <>
                        <del className="text-muted-foreground">
                          <span className="sr-only">{t("was")} </span>
                          {row.before}
                        </del>
                        <ArrowRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
                      </>
                    ) : null}
                    <ins className="font-semibold no-underline">
                      <span className="sr-only">{t("now")} </span>
                      {row.after}
                    </ins>
                    {row.change ? (
                      <span className="inline-flex items-center gap-1 rounded-full border border-(--hair) px-2 py-0.5 text-xs font-medium">
                        {row.change > 0 ? <TrendingUp aria-hidden className="size-3.5" /> : <TrendingDown aria-hidden className="size-3.5" />}
                        {t(row.change > 0 ? "up" : "down", { share: formatShare(Math.abs(row.change), locale) })}
                      </span>
                    ) : null}
                  </dd>
                </div>
              ))}
            </dl>
          </div>
        ) : null}
      </section>
      <button
        type="button"
        aria-expanded={open}
        aria-controls={moreId}
        onClick={() => setOpen((on) => !on)}
        className="inline-flex min-h-11 items-center gap-1 self-start rounded-full px-2 text-sm font-medium underline-offset-4 hover:underline focus-visible:ring-2 focus-visible:ring-(--gold) focus-visible:outline-none"
      >
        <ChevronDown aria-hidden className={`size-4 transition-transform duration-150 motion-reduce:transition-none ${open ? "rotate-180" : ""}`} />
        {t(open ? "fewerDetails" : "moreDetails")}
      </button>
      {/* Mounted only when open: the details list measures its rows on mount. */}
      <div id={moreId}>{open ? children : null}</div>
    </div>
  )
}
