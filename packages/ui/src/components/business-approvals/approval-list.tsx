"use client"

import type * as React from "react"

import { formatMoney } from "../../lib/money"
import { Badge } from "../badge"
import { EmptyState } from "../empty-state"
import { label } from "./labels"
import { orderName } from "./purchase-approval-card"
import type { ApprovalRequestView, Translate } from "./types"

/** Requests as a list to pick from; the picked one is marked for readers too. */
export function ApprovalList({
  items,
  selectedId,
  onSelect,
  empty,
  t,
  locale,
}: {
  items: ApprovalRequestView[]
  selectedId?: string
  onSelect(id: string): void
  empty: React.ReactNode
  t: Translate
  locale: string
}) {
  if (!items.length)
    return typeof empty === "string" ? (
      <EmptyState title={empty} />
    ) : (
      <>{empty}</>
    )
  return (
    <ul className="flex flex-col gap-2" aria-label={t("list.title")}>
      {items.map((r) => (
        <li key={r.id}>
          <button
            type="button"
            aria-current={r.id === selectedId ? "true" : undefined}
            onClick={() => onSelect(r.id)}
            className="flex min-h-11 w-full flex-col gap-1 rounded-lg border px-3 py-2 text-left transition-colors hover:bg-muted focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none aria-[current=true]:border-primary aria-[current=true]:bg-muted"
          >
            <span className="flex w-full flex-wrap items-center justify-between gap-2">
              <span className="font-medium">{orderName(r, t)}</span>
              <span className="font-medium tabular-nums">
                {formatMoney(
                  r.amount.minor,
                  r.amount.decimals,
                  r.amount.currency_ref,
                  locale
                )}
              </span>
            </span>
            <span className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
              <Badge
                variant={
                  r.status === "pending"
                    ? "gold"
                    : r.status === "blocked"
                      ? "red"
                      : "slate"
                }
              >
                {label(t, "approvalStatus", r.status)}
              </Badge>
              {r.execution ? (
                <Badge variant="slate">
                  {label(t, "executionStatus", r.execution.status)}
                </Badge>
              ) : null}
              {r.requester?.name}
            </span>
          </button>
        </li>
      ))}
    </ul>
  )
}
