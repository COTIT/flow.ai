"use client"

import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import {
  getAttentionPrioritiesQueryKey,
  getAttentionSummaryQueryKey,
  useAttentionMarksSet,
} from "@genesy/api-client/endpoints/attention"
import type { PriorityGroup } from "@genesy/api-client/model"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatQty } from "@genesy/ui/lib/format"
import { usePriorities, useWorkspace } from "@/lib/use-scene-data"
import { DataState } from "./data-state"
import type { RegistryProps } from "./types"

/** Where "Review" goes for a kind; kinds without a scene get no review link. */
export const REVIEW: Partial<Record<string, string>> = {
  "stock.below_minimum": "/scenes/stock",
  "orders.open": "/scenes/orders",
}

function Group({ group }: { group: PriorityGroup }) {
  const t = useTranslations("scenes")
  const locale = useLocale()
  const ws = useWorkspace()
  const client = useQueryClient()
  const mark = useAttentionMarksSet({
    mutation: {
      onSettled: () =>
        Promise.all([
          client.invalidateQueries({ queryKey: getAttentionPrioritiesQueryKey(ws) }),
          client.invalidateQueries({ queryKey: getAttentionSummaryQueryKey(ws) }),
        ]),
    },
  })
  const can = (action: string) => group.actions.includes(action)
  const review = REVIEW[group.kind]
  const kindKey = `noticed.kind.${group.kind}`
  return (
    <li data-group={group.kind} className="flex flex-col gap-2 rounded-2xl border bg-card p-4">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h3 className="font-semibold">{t.has(kindKey) ? t(kindKey, { count: group.count }) : group.kind}</h3>
        {group.impact.open_orders ? (
          <span className="text-xs text-muted-foreground">{t("noticed.affects", { count: group.impact.open_orders })}</span>
        ) : null}
      </div>
      {mark.isError ? <Notice variant="error" title={t("saveFailed")} /> : null}
      <ul className="flex flex-col gap-2">
        {group.items.map((item) => (
          <li key={item.ref_id} data-item={item.ref_id} className="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
            <span className="mr-auto font-medium">{item.label}</span>
            {item.available !== null && item.minimum !== null ? (
              <span className="flex gap-3 text-xs text-muted-foreground tabular-nums">
                <span>
                  {t("available")} {formatQty(item.available, locale)}
                </span>
                <span>
                  {t("minimum")} {formatQty(item.minimum, locale)}
                </span>
                {item.shortfall !== null ? (
                  <span className="font-medium text-foreground">
                    {t("shortfall")} {formatQty(item.shortfall, locale)}
                  </span>
                ) : null}
              </span>
            ) : null}
            <span className="flex flex-wrap gap-1">
              {can("acknowledge") ? (
                item.acknowledged ? (
                  <span data-acknowledged="" className="px-2 text-xs text-muted-foreground">
                    {t("noticed.acknowledged")}
                  </span>
                ) : (
                  <Button
                    variant="ghost"
                    size="sm"
                    className="min-h-11"
                    disabled={mark.isPending}
                    onClick={() => mark.mutate({ workspace: ws, data: { kind: group.kind, ref_id: item.ref_id, state: "acknowledged" } })}
                  >
                    {t("noticed.acknowledge")}
                  </Button>
                )
              ) : null}
              {can("snooze")
                ? (["until_tomorrow", "seven_days"] as const).map((snooze) => (
                    <Button
                      key={snooze}
                      variant="ghost"
                      size="sm"
                      className="min-h-11"
                      disabled={mark.isPending}
                      onClick={() => mark.mutate({ workspace: ws, data: { kind: group.kind, ref_id: item.ref_id, state: "snoozed", snooze } })}
                    >
                      {t(snooze === "until_tomorrow" ? "noticed.snoozeTomorrow" : "noticed.snoozeWeek")}
                    </Button>
                  ))
                : null}
            </span>
          </li>
        ))}
      </ul>
      {review && can("review") ? (
        <Link href={review} className={buttonVariants({ variant: "outline", size: "sm", className: "min-h-11 self-start" })}>
          {t(group.kind === "orders.open" ? "noticed.reviewOrders" : "noticed.reviewStock")}
        </Link>
      ) : null}
    </li>
  )
}

/** Flow noticed in full: every readable group, its items, and only the actions the API offers. */
export function AttentionPanel(_: RegistryProps) {
  const t = useTranslations("scenes")
  const query = usePriorities()
  const groups = (query.data?.data.groups ?? []).filter((g) => g.count > 0)
  return (
    <DataState query={query} empty={!groups.length} emptyTitle={t("noticed.calm")}>
      <ul data-attention="" className="flex flex-col gap-3">
        {groups.map((g) => (
          <Group key={g.kind} group={g} />
        ))}
      </ul>
    </DataState>
  )
}
