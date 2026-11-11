"use client"

import { useEffect, useRef } from "react"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import {
  useAttentionPriorities,
  useAttentionSummary,
} from "@genesy/api-client/endpoints/attention"
import {
  useOrdersList,
  useOrdersSalesCompare,
  useOrdersSalesDaily,
  useOrdersSalesSummary,
} from "@genesy/api-client/endpoints/orders"
import { useScheduledReportsList } from "@genesy/api-client/endpoints/scheduled-reports"
import type { FlowPeriod } from "@genesy/api-client/model"
import { useVoice } from "@genesy/voice/components/provider"
import {
  comparePeriod,
  localToday,
  midnightIn,
  periodRange,
  probeRange,
  TTL,
} from "./scene-data"
import { retryDelay, shouldRetry } from "./scene-errors"

const live = (ms: number) => ({
  staleTime: ms,
  refetchInterval: ms,
  refetchIntervalInBackground: false,
  // Flow's own rules (the shared client keeps TanStack's defaults): a refusal is not retried.
  retry: shouldRetry,
  retryDelay,
})

export const useWorkspace = () => useSession((s) => s.workspaceId) ?? ""

/**
 * Today in the workspace's reporting zone. The daily sales route says which zone it uses; a
 * caller without sales access (or before it answers) gets UTC, never the browser's zone.
 */
export function useWorkspaceToday(enabled = true) {
  const ws = useWorkspace()
  const probe = probeRange(new Date())
  const zone = useOrdersSalesDaily(ws, probe, {
    // Only asked when a sales view will show: a role without sales access never calls it.
    query: { enabled: enabled && !!ws, staleTime: Infinity, retry: false },
  })
  const timeZone = zone.data?.data.time_zone ?? null
  return {
    today: localToday(new Date(), timeZone),
    timeZone,
    ready: !zone.isPending,
  }
}

export function useAttention() {
  const ws = useWorkspace()
  return useAttentionSummary(ws, {
    query: { enabled: !!ws, ...live(TTL.attention) },
  })
}

export function usePriorities() {
  const ws = useWorkspace()
  return useAttentionPriorities(ws, {
    query: { enabled: !!ws, ...live(TTL.attention) },
  })
}

export function useSales(period: FlowPeriod, enabled = true) {
  const ws = useWorkspace()
  const { today, timeZone, ready } = useWorkspaceToday(enabled)
  const { from, to } = periodRange(period, today)
  // sales-summary takes instants, not days: the workspace-local midnights.
  const range = {
    from: midnightIn(from, timeZone),
    to: midnightIn(to, timeZone),
  }
  return useOrdersSalesSummary(ws, range, {
    query: { enabled: enabled && !!ws && ready, ...live(TTL.sales) },
  })
}

export function useCompare(period: FlowPeriod, enabled = true) {
  const ws = useWorkspace()
  const { today, ready } = useWorkspaceToday(enabled)
  const base = comparePeriod(period, today)
  return useOrdersSalesCompare(
    ws,
    {
      ...periodRange(period, today),
      compare_from: base.from,
      compare_to: base.to,
    },
    { query: { enabled: enabled && !!ws && ready, ...live(TTL.sales) } }
  )
}

export function useSeries(period: FlowPeriod, enabled = true) {
  const ws = useWorkspace()
  const { today, ready } = useWorkspaceToday(enabled)
  return useOrdersSalesDaily(ws, periodRange(period, today), {
    query: { enabled: enabled && !!ws && ready, ...live(TTL.series) },
  })
}

export function useOpenOrders(enabled = true) {
  const ws = useWorkspace()
  return useOrdersList(
    ws,
    { status: "open", page_size: 10 },
    {
      query: { enabled: enabled && !!ws, ...live(TTL.orders) },
    }
  )
}

export function useReports() {
  const ws = useWorkspace()
  return useScheduledReportsList(ws, {
    query: { enabled: !!ws, ...live(TTL.reports) },
  })
}

/** Business data the scenes read (never the saved layout, which only our own writes change). */
const DATA = ["attention", "orders", "sales", "scheduled-reports"]
export const isSceneData = (key: readonly unknown[]) =>
  typeof key[0] === "string" &&
  DATA.some((part) => (key[0] as string).includes(part))

/** Refresh: refetch every scene's data now. No voice, no language model. */
export function useRefreshScenes() {
  const client = useQueryClient()
  return () =>
    client.invalidateQueries({ predicate: (q) => isSceneData(q.queryKey) })
}

/** A confirmed voice action changed business data: show the new numbers everywhere. */
export function useRefreshOnOutcome() {
  const refresh = useRefreshScenes()
  const outcome = useVoice().outcome
  // The outcome present at mount was already shown before this scene opened: only a new one refreshes.
  const seen = useRef(outcome)
  useEffect(() => {
    if (!outcome || outcome === seen.current) return
    seen.current = outcome
    void refresh()
    // `refresh` is a new closure each render over a stable query client; depending on it would
    // refetch on every render, so only the outcome triggers.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [outcome])
}
