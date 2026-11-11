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
import { buttonVariants } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatQty } from "@genesy/ui/lib/format"
import { usePriorities, useWorkspace } from "@/lib/use-scene-data"
import { useSyncGuard } from "@/lib/use-sync-guard"
import { DataState, Restricted } from "./data-state"
import { RecordTable } from "./record-table"
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
          client.invalidateQueries({
            queryKey: getAttentionPrioritiesQueryKey(ws),
          }),
          client.invalidateQueries({
            queryKey: getAttentionSummaryQueryKey(ws),
          }),
        ]),
    },
  })
  const once = useSyncGuard()
  const send = (data: Parameters<typeof mark.mutate>[0]["data"]) =>
    // A second click in the same frame must not send the same write again.
    void once(() => mark.mutateAsync({ workspace: ws, data }).catch(() => null))
  const can = (action: string) => group.actions.includes(action)
  const review = REVIEW[group.kind]
  const kindKey = `noticed.kind.${group.kind.replace(".", "_")}`
  const heading = t.has(kindKey)
    ? t(kindKey, { count: group.count })
    : group.kind
  // Columns follow what the API returns for the group: quantities only where it sends them.
  const quantities = group.items.some(
    (i) => i.available !== null && i.minimum !== null
  )
  const qty = (n: string | null) => (n === null ? "" : formatQty(n, locale))
  return (
    <li
      data-group={group.kind}
      className="flex flex-col gap-3 rounded-2xl border bg-card p-4 @max-[599px]:border-0 @max-[599px]:bg-transparent @max-[599px]:p-0 @min-[980px]:p-5"
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="font-semibold">{heading}</h2>
        {group.impact.open_orders ? (
          <span className="text-xs text-muted-foreground">
            {t("noticed.affects", { count: group.impact.open_orders })}
          </span>
        ) : null}
      </div>
      {mark.isError ? <Notice variant="error" title={t("saveFailed")} /> : null}
      <RecordTable
        label={heading}
        rows={group.items}
        rowKey={(item) => item.ref_id}
        name={(item) => item.label}
        nameLabel={t(quantities ? "table.product" : "table.item")}
        columns={
          quantities
            ? [
                {
                  key: "available",
                  label: t("available"),
                  numeric: true,
                  value: (i) => qty(i.available),
                },
                {
                  key: "minimum",
                  label: t("minimum"),
                  numeric: true,
                  value: (i) => qty(i.minimum),
                },
                {
                  key: "shortfall",
                  label: t("shortfall"),
                  numeric: true,
                  strong: true,
                  value: (i) => qty(i.shortfall),
                },
              ]
            : []
        }
        actions={(item) => {
          const markAs = (
            data:
              | { state: "acknowledged" }
              | { state: "snoozed"; snooze: "until_tomorrow" | "seven_days" }
          ) =>
            // P2-3a: through the same-frame guard, never mark.mutate directly.
            send({ kind: group.kind, ref_id: item.ref_id, ...data })
          const snooze = can("snooze")
            ? {
                label: t("noticed.snooze"),
                items: [
                  {
                    key: "tomorrow",
                    label: t("noticed.snoozeTomorrow"),
                    short: t("noticed.untilTomorrow"),
                    onSelect: () =>
                      markAs({ state: "snoozed", snooze: "until_tomorrow" }),
                  },
                  {
                    key: "week",
                    label: t("noticed.snoozeWeek"),
                    short: t("noticed.forWeek"),
                    onSelect: () =>
                      markAs({ state: "snoozed", snooze: "seven_days" }),
                  },
                ],
              }
            : undefined
          const ack = can("acknowledge")
          if (!ack && !snooze) return null
          return {
            disabled: mark.isPending,
            status:
              ack && item.acknowledged ? t("noticed.acknowledged") : undefined,
            inline: ack
              ? {
                  key: "ack",
                  label: t("noticed.acknowledge"),
                  onSelect: () => markAs({ state: "acknowledged" }),
                }
              : undefined,
            group: snooze,
          }
        }}
      />
      {review && can("review") ? (
        <Link
          href={review}
          className={buttonVariants({
            variant: "outline",
            size: "sm",
            className: "min-h-11 self-start",
          })}
        >
          {t(
            group.kind === "orders.open"
              ? "noticed.reviewOrders"
              : "noticed.reviewStock"
          )}
        </Link>
      ) : null}
    </li>
  )
}

/** Flow noticed in full: every readable group, its items, and only the actions the API offers. */
export function AttentionPanel({ restricted }: RegistryProps) {
  const t = useTranslations("scenes")
  // Restricted: say so and never ask (an empty answer would read as "all clear").
  if (restricted) return <Restricted title={t("noticed.label")} />
  return <Priorities />
}

function Priorities() {
  const t = useTranslations("scenes")
  const query = usePriorities()
  const groups = (query.data?.data.groups ?? []).filter((g) => g.count > 0)
  return (
    <DataState
      query={query}
      empty={!groups.length}
      emptyTitle={t("noticed.calm")}
    >
      {/* The list's container: its width picks table, row-menu or card form (Lists board). */}
      <ul data-attention="" className="@container flex flex-col gap-3">
        {groups.map((g) => (
          <Group key={g.kind} group={g} />
        ))}
      </ul>
    </DataState>
  )
}
