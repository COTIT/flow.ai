"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { ShoppingBag, Undo2 } from "lucide-react"

import { useSession } from "@genesy/api-client/auth"
import { useLotsFarmerCredits } from "@genesy/api-client/endpoints/lots"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Chip, Tile } from "@/components/bento"
import { ListPager } from "@/components/list"
import { Empty, LoadError } from "@/components/states"

/** A village group's farmer count; the API hides it (null) below 3 farmers. */
export function FarmerCount({ farmers }: { farmers: number | null }) {
  const t = useTranslations("credits")
  return farmers == null ? t("fewFarmers") : t("farmers", { count: farmers })
}

/**
 * The batch's farmer groups as the public page shows them (OfficeLot rows: village, count,
 * harvest window; no names, bars or shares), then the sales and returns that credited them.
 * The API never sends farmer names, kg, prices or amounts here (G10).
 */
export function FarmerCredits({ lotId }: { lotId: string }) {
  const t = useTranslations("credits")
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [page, setPage] = React.useState(1)
  const credits = useLotsFarmerCredits(
    workspaceId,
    lotId,
    { page },
    { query: { enabled: Boolean(workspaceId) } }
  )
  // Harvest dates are calendar days: shown as given, not shifted by the browser's zone.
  const day = new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeZone: "UTC",
  })

  return (
    <Tile
      id="lot-credits"
      title={t("title")}
      span="col-span-5"
      action={<span className="text-xs text-muted-foreground">{t("sub")}</span>}
    >
      {credits.isPending ? (
        <div aria-busy="true" className="flex flex-col gap-3">
          {Array.from({ length: 3 }, (_, i) => (
            <Skeleton key={i} className="h-9 rounded-xl" />
          ))}
        </div>
      ) : credits.isError ? (
        <LoadError
          error={credits.error}
          onRetry={() => void credits.refetch()}
        />
      ) : (
        <>
          {credits.data.farmers.length ? (
            <ul className="flex flex-col gap-2">
              {credits.data.farmers.map((g) => (
                <li key={g.village} className="flex items-center gap-3">
                  <span
                    aria-hidden="true"
                    className="inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-primary text-[13px] font-semibold text-primary-foreground"
                  >
                    {g.village.slice(0, 2).toUpperCase()}
                  </span>
                  <div className="min-w-0 grow">
                    <p className="text-sm font-semibold">{g.village}</p>
                    <p className="text-xs text-muted-foreground">
                      <FarmerCount farmers={g.farmers} />
                      {g.harvest_start || g.harvest_end
                        ? ` · ${[g.harvest_start, g.harvest_end]
                            .filter((d) => d != null)
                            .map((d) => day.format(new Date(d)))
                            .join(" – ")}`
                        : null}
                    </p>
                  </div>
                </li>
              ))}
            </ul>
          ) : (
            <Notice title={t("noGroups")} />
          )}
          {credits.data.data.length === 0 ? (
            <Empty title={t("empty")} />
          ) : (
            <>
              <ul className="flex flex-col gap-2">
                {credits.data.data.map((c) => (
                  <li
                    key={`${c.order_id}-${c.kind}-${c.at}`}
                    className="flex items-center gap-3"
                  >
                    <Chip
                      icon={c.kind === "return" ? Undo2 : ShoppingBag}
                      tone={c.kind === "return" ? "gold" : "teal"}
                    />
                    <div className="min-w-0 grow">
                      <p className="text-sm font-semibold">
                        <Link
                          className="underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
                          href={`/orders/${c.order_id}`}
                        >
                          {c.order_number}
                        </Link>
                        {c.till_ref ? ` · ${t("till")}` : ""}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {t.has(`kinds.${c.kind}`)
                          ? t(`kinds.${c.kind}`)
                          : c.kind}
                        {" · "}
                        <span className="tabular-nums">
                          {new Date(c.at).toLocaleString()}
                        </span>
                      </p>
                    </div>
                    {/* Units as the API states them; never summed here. */}
                    <span className="text-sm font-semibold tabular-nums">
                      {t("jarCount", { jars: c.jars })}
                    </span>
                  </li>
                ))}
              </ul>
              <ListPager meta={credits.data.meta} onPage={setPage} />
            </>
          )}
        </>
      )}
    </Tile>
  )
}
