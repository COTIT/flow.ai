"use client"

import * as React from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { useSession } from "./auth"
import { useIdempotencyKey } from "./idempotency"
import { apiRequest } from "./mutator"

/**
 * Business approvals (WF-01) data, shared by Office and Flow: approval requests, their NetSuite
 * executions, and the Odoo/NetSuite connections, pairs and mappings behind them
 * (docs/integrations/contracts.md §5). The API decides everything; these only fetch and send.
 * shortcut: hand-typed against the contract while C1/C2 are unmerged; replace with the
 * orval-generated endpoints (K23) once apps/api/openapi.json carries these routes.
 */

type List<T> = {
  data: T[]
  meta: {
    current_page: number
    from: number | null
    last_page: number
    per_page: number
    to: number | null
    total: number
  }
}
type One<T> = { data: T }

const ROOT = ["business-approvals"] as const

function useWorkspace() {
  return useSession((s) => s.workspaceId) ?? ""
}

const get = <T>(url: string, params?: Record<string, unknown>) =>
  apiRequest<T>({ url, method: "GET", params })
const send = <T>(
  url: string,
  data?: unknown,
  headers?: Record<string, string>,
  method = "POST"
) => apiRequest<T>({ url, method, data }, headers ? { headers } : undefined)

/** Requests in a scope (`decidable` for the caller's to-do, `all` for oversight). */
export function useApprovalRequests<T>(params: {
  scope: "all" | "mine" | "decidable"
  status?: string
}) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "requests", params],
    enabled: Boolean(ws),
    queryFn: () =>
      get<List<T>>(`/workspace/${ws}/approvals/requests`, {
        workflow_key: "wf01_purchase",
        ...params,
      }),
  })
}

export function useApprovalRequest<T>(id: string | null | undefined) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "request", id],
    enabled: Boolean(ws && id),
    queryFn: () => get<One<T>>(`/workspace/${ws}/approvals/requests/${id}`),
  })
}

export type Decision = "approve" | "reject" | "return"

/**
 * Approve, reject or return. Sends the fingerprint the screen showed, so a request that
 * changed since answers 409 `approval_stale` instead of deciding something unseen.
 */
export function useDecide<T>() {
  const ws = useWorkspace()
  const client = useQueryClient()
  const key = useIdempotencyKey()
  return useMutation({
    mutationFn: (v: {
      id: string
      decision: Decision
      fingerprint: string
      comment?: string
    }) =>
      send<One<T>>(
        `/workspace/${ws}/approvals/requests/${v.id}/${v.decision}`,
        {
          fingerprint: v.fingerprint,
          ...(v.comment ? { comment: v.comment } : {}),
        },
        key.request.headers
      ),
    onSettled: (_d, error) => {
      key.settle(error)
      return client.invalidateQueries({ queryKey: [...ROOT, ws] })
    },
  })
}

/** The full execution (with `allowed_actions`) behind a request's summary. */
export function useExecution<T>(id: string | null | undefined) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "execution", id],
    enabled: Boolean(ws && id),
    queryFn: () => get<One<T>>(`/workspace/${ws}/workflows/executions/${id}`),
  })
}

/** `retry` (after a fix) or a reconciliation `resolve` action, each with a reason. */
export function useExecutionAction<T>() {
  const ws = useWorkspace()
  const client = useQueryClient()
  const key = useIdempotencyKey()
  return useMutation({
    mutationFn: (v: {
      id: string
      action: string
      reason: string
      targetId?: string
    }) =>
      v.action === "retry"
        ? send<One<T>>(
            `/workspace/${ws}/workflows/executions/${v.id}/retry`,
            { reason: v.reason },
            key.request.headers
          )
        : send<One<T>>(
            `/workspace/${ws}/workflows/executions/${v.id}/resolve`,
            {
              action: v.action,
              reason: v.reason,
              ...(v.targetId ? { target_id: v.targetId } : {}),
            },
            key.request.headers
          ),
    onSettled: (_d, error) => {
      key.settle(error)
      return client.invalidateQueries({ queryKey: [...ROOT, ws] })
    },
  })
}

// --- Office settings: connections, pairs, mappings ---------------------------------------

export function useIntegrationList<T>(
  path: "connections" | "pairs",
  enabled = true
) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, path],
    enabled: Boolean(ws) && enabled,
    queryFn: () => get<List<T>>(`/workspace/${ws}/integrations/${path}`),
  })
}

export function useMappings<T>(pair: string, entity?: string) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "mappings", pair, entity ?? ""],
    enabled: Boolean(ws && pair),
    queryFn: () =>
      get<List<T>>(
        `/workspace/${ws}/integrations/pairs/${pair}/mappings`,
        entity ? { entity } : undefined
      ),
  })
}

/**
 * One integrations write, at `/workspace/{ws}/integrations/{path}`; refreshes this
 * feature's reads when it settles. Secrets travel only in the body, never the URL.
 */
export function useIntegrationWrite<T = unknown>() {
  const ws = useWorkspace()
  const client = useQueryClient()
  return useMutation({
    mutationFn: (v: {
      path: string
      data?: unknown
      method?: "POST" | "DELETE"
    }) =>
      send<T>(
        `/workspace/${ws}/integrations/${v.path}`,
        v.data,
        undefined,
        v.method ?? "POST"
      ),
    onSettled: () => client.invalidateQueries({ queryKey: [...ROOT, ws] }),
  })
}

/**
 * The approvals screen's state, shared by Office and Flow: scope, the picked request, its
 * full execution, and the last outcome. `done` is what just succeeded; errors stay raw for the
 * app to put into words. A stale (409) decision is cleared by `reload`, which re-reads the
 * request so the next decision carries the new fingerprint.
 */
export function usePurchaseApprovals<
  R extends {
    id: string
    fingerprint: string
    execution: { id: string } | null
  },
  E,
>() {
  const [scope, setScope] = React.useState<"decidable" | "all">("decidable")
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [done, setDone] = React.useState<
    "approved" | "rejected" | "returned" | "acted" | null
  >(null)
  const list = useApprovalRequests<R>({ scope })
  const detail = useApprovalRequest<R>(selectedId)
  const request = detail.data?.data
  const execution = useExecution<E>(request?.execution?.id)
  const decide = useDecide<R>()
  const act = useExecutionAction<E>()
  const outcome = {
    approve: "approved",
    reject: "rejected",
    return: "returned",
  } as const
  return {
    scope,
    setScope: (s: "decidable" | "all") => {
      setScope(s)
      setSelectedId(null)
    },
    list,
    selectedId,
    select: (id: string | null) => {
      setSelectedId(id)
      setDone(null)
      decide.reset()
      act.reset()
    },
    detail,
    request,
    execution,
    busy: decide.isPending || act.isPending,
    error: decide.error ?? act.error,
    done,
    decide: (decision: Decision, comment?: string) => {
      if (!request) return
      setDone(null)
      act.reset()
      decide.mutate(
        { id: request.id, decision, fingerprint: request.fingerprint, comment },
        { onSuccess: () => setDone(outcome[decision]) }
      )
    },
    act: (action: string, reason: string, targetId?: string) => {
      const id = request?.execution?.id
      if (!id) return
      setDone(null)
      decide.reset()
      act.mutate(
        { id, action, reason, targetId },
        { onSuccess: () => setDone("acted") }
      )
    },
    reload: () => {
      decide.reset()
      act.reset()
      void detail.refetch()
      if (request?.execution) void execution.refetch()
    },
  }
}
