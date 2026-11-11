"use client"

import { useSyncExternalStore } from "react"
import { useTranslations } from "next-intl"
import { Lock } from "lucide-react"

import { Button } from "@genesy/ui/components/button"
import { EmptyState } from "@genesy/ui/components/empty-state"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { freshness } from "@/lib/freshness"

/** The part of a TanStack query a component's states depend on. */
export type Query = {
  isPending: boolean
  isError: boolean
  isFetching: boolean
  dataUpdatedAt: number
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
  children,
}: {
  query: Query
  empty: boolean
  emptyTitle: string
  children: React.ReactNode
}) {
  const t = useTranslations("scenes")
  if (query.isPending) return <Skeleton data-loading="" className="h-24 w-full rounded-2xl" />
  if (query.isError && !query.dataUpdatedAt)
    return (
      <Notice
        variant="error"
        title={t("failed")}
        action={
          <Button variant="outline" size="sm" onClick={() => void query.refetch()}>
            {t("retry")}
          </Button>
        }
      />
    )
  if (empty) return <EmptyState title={emptyTitle} className="py-6" />
  return <>{children}</>
}

const tick = (notify: () => void) => {
  const id = setInterval(notify, 1000)
  return () => clearInterval(id)
}

/** "Updated 32 s ago" / "Refreshing…" / "Data may be outdated" for the queries behind a scene. */
export function Freshness({ queries, staleAfter }: { queries: Query[]; staleAfter: number }) {
  const t = useTranslations("scenes")
  const now = useSyncExternalStore(tick, () => Math.floor(Date.now() / 1000) * 1000, () => 0)
  const answered = queries.filter((q) => q.dataUpdatedAt)
  if (!now || !answered.length) return null
  const state = freshness({
    updatedAt: Math.min(...answered.map((q) => q.dataUpdatedAt)),
    fetching: queries.some((q) => q.isFetching),
    failed: queries.some((q) => q.isError),
    now,
    staleAfter,
  })
  const text = {
    loading: "",
    fresh: t("fresh", { seconds: state.seconds }),
    refreshing: t("refreshing"),
    stale: t("stale"),
    failed: t("refreshFailed"),
  }[state.kind]
  return (
    <p data-freshness={state.kind} aria-live="polite" className="text-xs text-muted-foreground">
      {text}
    </p>
  )
}
