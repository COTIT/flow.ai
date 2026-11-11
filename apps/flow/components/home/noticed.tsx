"use client"

import Link from "next/link"
import { useTranslations } from "next-intl"

import { buttonVariants } from "@genesy/ui/components/button"
import { usePriorities } from "@/lib/use-scene-data"

/** Home's Flow noticed card: the top two groups, grounded counts, one way in. No conversation needed. */
export function Noticed() {
  const t = useTranslations("scenes")
  const query = usePriorities()
  // Snoozed groups never lead on home; the API already orders the rest.
  const groups = (query.data?.data.groups ?? []).filter(
    (g) => g.count > 0 && !g.snoozed
  )
  if (!query.data) return null
  const top = groups.slice(0, 2)
  const impact = top[0]?.impact.open_orders
  return (
    <section
      aria-labelledby="flow-noticed-label"
      className="flex w-full flex-col gap-2 rounded-2xl border bg-card/70 p-4 text-left"
    >
      <h2
        id="flow-noticed-label"
        className="text-xs font-semibold tracking-wide text-muted-foreground uppercase"
      >
        {t("noticed.label")}
      </h2>
      {top.length ? (
        <>
          <p className="font-medium">
            {t("noticed.headline", { count: groups.length })}
            {impact ? ` ${t("noticed.impact", { count: impact })}` : null}
          </p>
          <ul className="flex flex-col gap-0.5 text-sm text-muted-foreground">
            {top.map((g) => {
              const key = `noticed.kind.${g.kind.replace(".", "_")}`
              return (
                <li key={g.kind}>
                  {t.has(key) ? t(key, { count: g.count }) : g.kind}
                </li>
              )
            })}
          </ul>
          <Link
            href="/scenes/noticed"
            className={buttonVariants({
              variant: "outline",
              size: "sm",
              className: "min-h-11 self-start",
            })}
          >
            {t("noticed.review")}
          </Link>
        </>
      ) : (
        <p className="text-sm text-muted-foreground">{t("noticed.calm")}</p>
      )}
    </section>
  )
}
