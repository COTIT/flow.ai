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
import { RecordTable, type RowAction } from "./record-table"
import { useOpenInOffice, useRestock } from "./row-steps"
import type { RegistryProps } from "./types"

/** The Office page that deals with one item of a kind (its record, or the list). */
const OFFICE: Partial<Record<string, (ref: string) => string>> = {
  "orders.open": (ref) => `/orders/${ref}`,
  "invoices.overdue": (ref) => `/invoices/${ref}`,
  "lots.quarantined": (ref) => `/lots/${ref}`,
  "till.open_shifts": () => "/till/shifts",
  "webhooks.failed": () => "/settings/webhooks",
}

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
  const restock = useRestock()
  const office = useOpenInOffice()
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
      className="flex flex-col gap-3 rounded-2xl border bg-card p-4 @max-[519px]:border-0 @max-[519px]:bg-transparent @max-[519px]:p-0 @min-[940px]:p-5"
    >
      {/* One title row: the title (and what it affects) left, the way to act right. */}
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <h2 className="font-semibold">{heading}</h2>
          {group.impact.open_orders ? (
            <p className="text-xs text-muted-foreground">
              {t("noticed.affects", { count: group.impact.open_orders })}
            </p>
          ) : null}
        </div>
        {/* Lists board: the way to act sits in the group's header row, never under the list. */}
        {review && can("review") ? (
          <Link
            href={review}
            className={buttonVariants({
              variant: "outline",
              size: "sm",
              className: "min-h-11 shrink-0 rounded-full",
            })}
          >
            {t(
              group.kind === "orders.open"
                ? "noticed.reviewOrders"
                : "noticed.reviewStock"
            )}
          </Link>
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
          // The item's own next step: restock it, or open it where it is dealt with.
          const path = OFFICE[group.kind]?.(item.ref_id)
          const step: RowAction | undefined =
            group.kind === "stock.below_minimum"
              ? restock({
                  id: item.ref_id,
                  name: item.label,
                  shortfall: item.shortfall,
                })
              : path
                ? office(path)
                : undefined
          const ack = can("acknowledge") && !item.acknowledged
          const later = [
            ...(ack && step
              ? [
                  {
                    key: "ack",
                    label: t("noticed.acknowledge"),
                    onSelect: () => markAs({ state: "acknowledged" as const }),
                  },
                ]
              : []),
            ...(can("snooze")
              ? [
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
                ]
              : []),
          ]
          // No next step: Acknowledge stays the button, as before.
          const inline =
            step ??
            (ack
              ? {
                  key: "ack",
                  label: t("noticed.acknowledge"),
                  onSelect: () => markAs({ state: "acknowledged" }),
                }
              : undefined)
          if (!inline && !later.length) return null
          return {
            disabled: mark.isPending,
            status:
              can("acknowledge") && item.acknowledged
                ? t("noticed.acknowledged")
                : undefined,
            inline,
            group: later.length
              ? {
                  label: t(step ? "noticed.later" : "noticed.snooze"),
                  items: later,
                }
              : undefined,
          }
        }}
      />
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
