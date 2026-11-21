"use client"

import * as React from "react"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { useSession } from "./auth"
import {
  approvalsRequestsApprove,
  approvalsRequestsGet,
  approvalsRequestsList,
  approvalsRequestsReject,
  approvalsRequestsReturn,
} from "./generated/approvals/approvals"
import {
  integrationsConnectionsList,
  integrationsMappingsList,
  integrationsPairsList,
} from "./generated/integrations/integrations"
import {
  workflowsExecutionsGet,
  workflowsExecutionsResolve,
  workflowsExecutionsRetry,
  workflowsIncidentsGet,
  workflowsIncidentsList,
  workflowsIncidentsResolve,
  workflowsIncidentsVerify,
} from "./generated/workflows/workflows"
import { useIdempotencyKey } from "./idempotency"

/**
 * Business approvals (WF-01) data, shared by Office and Flow: approval requests, their NetSuite
 * executions, and the Odoo/NetSuite connections, pairs and mappings behind them
 * (docs/integrations/contracts.md §5). The API decides everything; these only fetch and send.
 * Calls go through the generated endpoints. The generated models type every enum as `string`,
 * so callers pass their narrower view type (`T`) and the reply is read as that
 * (@verzo/ui business-approvals/types.ts, checked against the generated models there).
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
// The generated reply, read as the caller's view type (same JSON, narrower enums).
const as = <T>(p: Promise<unknown>) => p as Promise<T>

function useWorkspace() {
  return useSession((s) => s.workspaceId) ?? ""
}

/** Requests in a scope (`decidable` for the caller's to-do, `all` for oversight). */
export function useApprovalRequests<T>(params: {
  scope: "all" | "mine" | "decidable"
  status?: string
}) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "requests", params],
    enabled: Boolean(ws),
    queryFn: ({ signal }) =>
      as<List<T>>(
        approvalsRequestsList(
          ws,
          { workflow_key: "wf01_purchase", ...params },
          undefined,
          signal
        )
      ),
  })
}

export function useApprovalRequest<T>(id: string | null | undefined) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "request", id],
    enabled: Boolean(ws && id),
    queryFn: ({ signal }) =>
      as<One<T>>(approvalsRequestsGet(ws, id!, undefined, signal)),
  })
}

export type Decision = "approve" | "reject" | "return"
const decide = {
  approve: approvalsRequestsApprove,
  reject: approvalsRequestsReject,
  return: approvalsRequestsReturn,
}

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
      as<One<T>>(
        decide[v.decision](
          ws,
          v.id,
          {
            fingerprint: v.fingerprint,
            ...(v.comment ? { comment: v.comment } : {}),
          },
          key.request
        )
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
    queryFn: ({ signal }) =>
      as<One<T>>(workflowsExecutionsGet(ws, id!, undefined, signal)),
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
      as<One<T>>(
        v.action === "retry"
          ? workflowsExecutionsRetry(
              ws,
              v.id,
              { reason: v.reason },
              key.request
            )
          : workflowsExecutionsResolve(
              ws,
              v.id,
              {
                action: v.action,
                reason: v.reason,
                ...(v.targetId ? { target_id: v.targetId } : {}),
              },
              key.request
            )
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
  const list =
    path === "connections" ? integrationsConnectionsList : integrationsPairsList
  return useQuery({
    queryKey: [...ROOT, ws, path],
    enabled: Boolean(ws) && enabled,
    queryFn: ({ signal }) =>
      as<List<T>>(list(ws, undefined, undefined, signal)),
  })
}

export function useMappings<T>(pair: string, entity?: string) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "mappings", pair, entity ?? ""],
    enabled: Boolean(ws && pair),
    queryFn: ({ signal }) =>
      as<List<T>>(
        integrationsMappingsList(
          ws,
          pair,
          entity ? { entity } : undefined,
          undefined,
          signal
        )
      ),
  })
}

/**
 * One integrations write: `call` gets the workspace and runs a generated endpoint
 * (e.g. `(ws) => integrationsConnectionsTest(ws, id)`); this feature's reads refresh after.
 * Secrets travel only in the body, never the URL.
 */
export function useIntegrationWrite<T = unknown>() {
  const ws = useWorkspace()
  const client = useQueryClient()
  return useMutation({
    mutationFn: (call: (workspace: string) => Promise<T>) => call(ws),
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

// --- Sync problems (WF-03 incidents, contracts §11.7) ------------------------------------

/** Incidents by status and category, newest first. */
export function useIncidents<T>(params: {
  status?: "open" | "resolved"
  category?: string
  page_size?: string
}) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "incidents", params],
    enabled: Boolean(ws),
    queryFn: ({ signal }) =>
      as<List<T>>(workflowsIncidentsList(ws, params, undefined, signal)),
  })
}

/** One incident with its watched expectation and timeline. */
export function useIncident<T>(id: string | null | undefined) {
  const ws = useWorkspace()
  return useQuery({
    queryKey: [...ROOT, ws, "incident", id],
    enabled: Boolean(ws && id),
    queryFn: ({ signal }) =>
      as<One<T>>(workflowsIncidentsGet(ws, id!, undefined, signal)),
  })
}

/** `verify` (one read-back now) or `resolve` (close by hand with a note). */
export function useIncidentAction<T>() {
  const ws = useWorkspace()
  const client = useQueryClient()
  const key = useIdempotencyKey()
  return useMutation({
    mutationFn: (v: {
      id: string
      action: "verify" | "resolve"
      note?: string
    }) =>
      as<One<T>>(
        v.action === "verify"
          ? workflowsIncidentsVerify(ws, v.id, key.request)
          : workflowsIncidentsResolve(
              ws,
              v.id,
              { note: v.note ?? "", resolution_kind: "manual_closed" },
              key.request
            )
      ),
    onSettled: (_d, error) => {
      key.settle(error)
      return client.invalidateQueries({ queryKey: [...ROOT, ws] })
    },
  })
}

/**
 * The sync problems screen (Office): open/closed and category filters, the selected
 * incident, and its Check now / Close actions. The API decides; this only fetches and sends.
 */
export function useIncidentsScreen<
  I extends { id: string; status: string },
  D = I,
>() {
  const [status, setStatus] = React.useState<"open" | "resolved">("open")
  const [category, setCategory] = React.useState<string | null>(null)
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [done, setDone] = React.useState<"verified" | "resolved" | null>(null)
  const list = useIncidents<I>({ status, ...(category ? { category } : {}) })
  const detail = useIncident<D>(selectedId)
  const act = useIncidentAction<I>()
  const choose = (id: string | null) => {
    setSelectedId(id)
    setDone(null)
    act.reset()
  }
  return {
    status,
    setStatus: (s: "open" | "resolved") => {
      setStatus(s)
      choose(null)
    },
    category,
    setCategory: (c: string | null) => {
      setCategory(c)
      choose(null)
    },
    list,
    selectedId,
    select: choose,
    detail,
    incident: detail.data?.data,
    busy: act.isPending,
    error: act.error,
    done,
    verify: () => {
      if (!selectedId) return
      setDone(null)
      act.mutate(
        { id: selectedId, action: "verify" },
        { onSuccess: () => setDone("verified") }
      )
    },
    resolve: (note: string) => {
      if (!selectedId) return
      setDone(null)
      act.mutate(
        { id: selectedId, action: "resolve", note },
        { onSuccess: () => setDone("resolved") }
      )
    },
  }
}
