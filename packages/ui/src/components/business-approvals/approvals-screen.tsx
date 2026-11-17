"use client"

import { ArrowLeft } from "lucide-react"

import { Button } from "../button"
import { EmptyState } from "../empty-state"
import { Notice } from "../notice"
import { Skeleton } from "../skeleton"
import { ApprovalList } from "./approval-list"
import { errorText } from "./labels"
import { PurchaseApprovalCard } from "./purchase-approval-card"
import type {
  ApprovalRequestView,
  BlockedReasonView,
  ExecutionView,
  ResolveAction,
  Translate,
} from "./types"

type Scope = "decidable" | "all"

/**
 * Purchase approvals as one screen, for Office and Flow: list beside detail on wide screens,
 * one at a time on a phone. Display only; the app feeds it `usePurchaseApprovals()`.
 * A stale decision (409 `approval_stale`) offers Reload, which brings the new version.
 */
export function ApprovalsScreen({
  s,
  t,
  locale,
  mappingHref,
}: {
  s: {
    scope: Scope
    setScope(scope: Scope): void
    list: {
      data?: { data: ApprovalRequestView[] }
      isPending: boolean
      isError: boolean
      error: unknown
      refetch(): unknown
    }
    selectedId: string | null
    select(id: string | null): void
    detail: { isPending: boolean; isError: boolean; error: unknown }
    request?: ApprovalRequestView
    execution: { data?: { data: ExecutionView } }
    busy: boolean
    error: unknown
    done: "approved" | "rejected" | "returned" | "acted" | null
    decide(decision: "approve" | "reject" | "return", comment?: string): void
    act(
      action: ResolveAction | "retry",
      reason: string,
      targetId?: string
    ): void
    reload(): void
  }
  t: Translate
  locale: string
  mappingHref?: (b: BlockedReasonView) => string | undefined
}) {
  const code = (s.error as { response?: { data?: { code?: string } } } | null)
    ?.response?.data?.code
  const stale = code === "approval_stale" || code === "execution_state_changed"
  const items = s.list.data?.data ?? []
  // The request's execution summary, with the full row's actions once it has loaded.
  const request = s.request && {
    ...s.request,
    execution:
      s.request.execution &&
      s.execution.data?.data.id === s.request.execution.id
        ? { ...s.request.execution, ...s.execution.data.data }
        : s.request.execution,
  }

  return (
    <div className="grid gap-4 md:grid-cols-[minmax(16rem,22rem)_1fr]">
      <section
        aria-label={t("list.title")}
        className={`flex flex-col gap-3 ${s.selectedId ? "max-md:hidden" : ""}`}
      >
        <div role="group" aria-label={t("list.title")} className="flex gap-2">
          {(["decidable", "all"] as const).map((scope) => (
            <Button
              key={scope}
              size="lg"
              className="min-h-11"
              variant={s.scope === scope ? "secondary" : "ghost"}
              aria-pressed={s.scope === scope}
              onClick={() => s.setScope(scope)}
            >
              {t(`list.${scope}`)}
            </Button>
          ))}
        </div>
        {s.list.isPending ? (
          <div className="flex flex-col gap-2" aria-label={t("list.loading")}>
            <Skeleton className="h-16" />
            <Skeleton className="h-16" />
          </div>
        ) : s.list.isError ? (
          <Notice
            variant="error"
            title={t("list.loadError")}
            action={
              <Button
                variant="outline"
                size="lg"
                onClick={() => void s.list.refetch()}
              >
                {t("list.retry")}
              </Button>
            }
          >
            {errorText(t, s.list.error)}
          </Notice>
        ) : (
          <ApprovalList
            items={items}
            selectedId={s.selectedId ?? undefined}
            onSelect={s.select}
            empty={
              <EmptyState
                title={t("list.empty")}
                description={t("list.emptyText")}
              />
            }
            t={t}
            locale={locale}
          />
        )}
      </section>

      <section
        aria-label={request ? undefined : t("list.choose")}
        className={`flex min-w-0 flex-col gap-4 ${s.selectedId ? "" : "max-md:hidden"}`}
      >
        {s.selectedId ? (
          <Button
            variant="ghost"
            size="lg"
            className="min-h-11 self-start md:hidden"
            onClick={() => s.select(null)}
          >
            <ArrowLeft aria-hidden="true" />
            {t("list.back")}
          </Button>
        ) : null}
        <div role="status">
          {s.done ? (
            <Notice
              variant="success"
              live={false}
              title={t(`done.${s.done}`)}
            />
          ) : null}
        </div>
        {stale ? (
          <Notice
            variant="warning"
            title={errorText(t, s.error)}
            action={
              <Button
                variant="outline"
                size="lg"
                className="min-h-11"
                onClick={s.reload}
              >
                {t("list.reload")}
              </Button>
            }
          />
        ) : null}
        {!s.selectedId ? (
          <EmptyState title={t("list.choose")} />
        ) : s.detail.isPending ? (
          <Skeleton className="h-64" />
        ) : s.detail.isError ? (
          <Notice variant="error" title={errorText(t, s.detail.error)} />
        ) : request ? (
          <PurchaseApprovalCard
            request={request}
            busy={s.busy}
            error={s.error && !stale ? errorText(t, s.error) : null}
            onApprove={(c) => s.decide("approve", c)}
            onReject={(c) => s.decide("reject", c)}
            onReturn={(c) => s.decide("return", c)}
            onAction={s.act}
            mappingHref={mappingHref}
            t={t}
            locale={locale}
          />
        ) : null}
      </section>
    </div>
  )
}

export type { ApprovalRequestView, ExecutionView }
