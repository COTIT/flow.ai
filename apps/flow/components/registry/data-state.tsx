"use client"

import { useState, useSyncExternalStore } from "react"
import { useFormatter, useTranslations } from "next-intl"
import { useQueryClient, type QueryCache } from "@tanstack/react-query"
import { Lock } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { ageLabel, freshness } from "@/lib/freshness"
import { freshnessClock } from "@/lib/freshness-clock"
import { sceneError } from "@/lib/scene-errors"
import { isSceneData } from "@/lib/use-scene-data"

/** The part of a TanStack query a component's states depend on. */
export type Query = {
  isPending: boolean
  isError: boolean
  isFetching: boolean
  dataUpdatedAt: number
  error?: unknown
  /** The last failed try while retries are still running. */
  failureReason?: unknown
  refetch: () => unknown
}

/** "Not available for your role": the API sent no value, so none is drawn (never a zero). */
export function Restricted({ title }: { title: string }) {
  const t = useTranslations("scenes")
  return (
    <div
      data-restricted=""
      className="flex items-center gap-2 rounded-2xl border border-dashed p-4 text-sm text-muted-foreground"
    >
      <Lock aria-hidden className="size-4 shrink-0" />
      <span>
        <span className="font-medium text-foreground">{title}</span>
        {" · "}
        {t("restricted")}
      </span>
    </div>
  )
}

/** Loading, failed (with retry), empty, or the content; refreshing keeps the content shown. */
export function DataState({
  query,
  empty,
  emptyTitle,
  reserve = 96,
  children,
}: {
  query: Query
  empty: boolean
  emptyTitle: string
  /** Height in px of the card this becomes: placeholders keep it, so nothing below jumps. */
  reserve?: number
  children: React.ReactNode
}) {
  const t = useTranslations("scenes")
  const waiting = sceneError(query.failureReason)
  if (query.isPending && waiting.kind === "busy")
    return (
      <Held px={reserve}>
        <Notice
          variant="info"
          title={t("busy", { seconds: Math.ceil(waiting.retryAfter) })}
        />
      </Held>
    )
  if (query.isPending)
    return (
      <Skeleton
        data-loading=""
        className="w-full rounded-2xl"
        style={{ height: reserve }}
      />
    )
  const failure = sceneError(query.error)
  if (query.isError && !query.dataUpdatedAt && failure.kind === "forbidden")
    return (
      <Held px={reserve}>
        <Notice variant="info" title={t("restricted")} />
      </Held>
    )
  if (query.isError && !query.dataUpdatedAt && failure.kind === "missing")
    return (
      <Held px={reserve}>
        <EmptyState title={t("gone")} className="py-6" />
      </Held>
    )
  if (query.isError && !query.dataUpdatedAt)
    return (
      <Held px={reserve}>
        <Notice
          variant="error"
          title={t("failed")}
          action={
            <Button
              variant="outline"
              size="sm"
              onClick={() => void query.refetch()}
            >
              {t("retry")}
            </Button>
          }
        />
      </Held>
    )
  if (empty)
    return (
      <Held px={reserve}>
        <EmptyState title={emptyTitle} className="py-6" />
      </Held>
    )
  return <>{children}</>
}

/** A state that stands in for a card keeps the card's height. */
function Held({ px, children }: { px: number; children: React.ReactNode }) {
  return <div style={{ minHeight: px }}>{children}</div>
}

/** Everything the line says, as one short key: a re-render happens only when it changes. */
function shown(
  q: { updatedAt: number; fetching: boolean; failed: boolean },
  staleAfter: number
) {
  if (!q.updatedAt) return null
  const now = Date.now()
  const state = freshness({ ...q, now, staleAfter })
  return { state, label: ageLabel(now - q.updatedAt) }
}

/** The queries behind the visible scene: when the oldest answered, and whether any is refetching or failed. */
function readScenes(cache: QueryCache) {
  const queries = cache.findAll({
    predicate: (q) => isSceneData(q.queryKey) && q.getObserversCount() > 0,
  })
  const answered = queries.filter((q) => q.state.dataUpdatedAt)
  return {
    updatedAt: answered.length
      ? Math.min(...answered.map((q) => q.state.dataUpdatedAt))
      : 0,
    fetching: queries.some((q) => q.state.fetchStatus === "fetching"),
    // A query that never answered (e.g. a role without that data) has no shown data to age.
    failed: answered.some((q) => q.state.status === "error"),
  }
}

/** "Updated just now" / "Updated 5 min ago" / "Refreshing…" / "Data may be outdated" for the data the visible scene shows. */
export function Freshness({ staleAfter }: { staleAfter: number }) {
  const t = useTranslations("scenes")
  const format = useFormatter()
  const cache = useQueryClient().getQueryCache()
  const input = readScenes(cache)
  // Re-render when a scene query changes (first answer, refetch) as well as when the clock ticks.
  // Stable identity: a new one each render would restart the clock's countdown.
  const [subscribe] = useState(() => (notify: () => void) => {
    const off = freshnessClock.subscribe(notify, () => {
      const { updatedAt } = readScenes(cache)
      return updatedAt ? Date.now() - updatedAt : 0
    })
    const unwatch = cache.subscribe(notify)
    return () => {
      off()
      unwatch()
    }
  })
  useSyncExternalStore(
    subscribe,
    () => {
      const live = readScenes(cache)
      const v = shown(live, staleAfter)
      return v && `${v.state.kind}/${JSON.stringify(v.label)}/${live.updatedAt}`
    },
    () => null
  )
  const view = shown(input, staleAfter)
  if (!view) return null
  const { state, label } = view
  const text = {
    loading: "",
    fresh: {
      now: () => t("freshNow"),
      minutes: () =>
        t("freshMinutes", {
          minutes: label.kind === "minutes" ? label.minutes : 0,
        }),
      at: () =>
        t("freshAt", {
          time: format.dateTime(new Date(input.updatedAt), {
            hour: "2-digit",
            minute: "2-digit",
          }),
        }),
    }[label.kind](),
    refreshing: t("refreshing"),
    stale: t("stale"),
    failed: t("refreshFailed"),
  }[state.kind]
  return (
    <p data-freshness={state.kind} className="text-xs text-muted-foreground">
      {text}
      {/* Only changes worth hearing: the quiet "Updated 5 min ago" is never announced. */}
      <span aria-live="polite" className="sr-only">
        {state.kind === "fresh" ? "" : text}
      </span>
    </p>
  )
}
