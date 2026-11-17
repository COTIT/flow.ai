"use client"

import * as React from "react"

import { formatMoney } from "../../lib/money"
import { Badge } from "../badge"
import { Button } from "../button"
import { Label } from "../label"
import { Notice } from "../notice"
import { Textarea } from "../textarea"
import { ExecutionStatus } from "./execution-status"
import { IncidentBadge } from "./incident-badge"
import { blockedReason, label } from "./labels"
import type {
  ApprovalRequestView,
  BlockedReasonView,
  ExecutionView,
  ResolveAction,
  Translate,
} from "./types"

export type {
  ApprovalRequestView,
  BlockedReasonView,
  ExecutionView,
  ResolveAction,
  Translate,
}

const statusTone = {
  pending: "gold",
  approved: "teal",
  blocked: "red",
  rejected: "red",
  returned: "slate",
  invalidated: "slate",
} as const

/**
 * One purchase request to decide: the Odoo snapshot (amounts are Odoo's estimate; NetSuite
 * calculates tax), its approval status, and apart from that its NetSuite order. Display only:
 * which buttons show comes from the API (`can_decide`, `allowed_actions`), never from rules here.
 */
export function PurchaseApprovalCard({
  request,
  onApprove,
  onReject,
  onReturn,
  onAction,
  busy,
  error,
  mappingHref,
  t,
  locale,
}: {
  request: ApprovalRequestView
  onApprove?(comment?: string): void
  onReject?(comment: string): void
  onReturn?(comment: string): void
  onAction?(
    action: ResolveAction | "retry",
    reason: string,
    targetId?: string
  ): void
  busy?: boolean
  error?: string | null
  mappingHref?: (b: BlockedReasonView) => string | undefined
  t: Translate
  locale: string
}) {
  const [comment, setComment] = React.useState("")
  const [needComment, setNeedComment] = React.useState(false)
  const commentRef = React.useRef<HTMLTextAreaElement>(null)
  const id = React.useId()
  const s = request.snapshot
  const decimals = s?.currency.decimals ?? request.amount.decimals
  const currency = s?.currency.ref ?? request.amount.currency_ref
  const money = (minor: number) =>
    formatMoney(minor, decimals, currency, locale)
  const needs = (send?: (c: string) => void) => () => {
    if (!comment.trim()) {
      setNeedComment(true)
      commentRef.current?.focus()
      return
    }
    send?.(comment.trim())
  }

  return (
    <article
      aria-labelledby={`${id}-title`}
      className="flex flex-col gap-5"
      data-testid="purchase-approval-card"
    >
      <header className="flex flex-col gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <h2 id={`${id}-title`} className="text-lg font-semibold text-balance">
            {orderName(request, t)}
          </h2>
          <Badge variant={statusTone[request.status] ?? "slate"}>
            {label(t, "approvalStatus", request.status)}
          </Badge>
          {request.open_incidents.map((i) => (
            <IncidentBadge key={i.id} kind={i.kind} status="open" t={t} />
          ))}
        </div>
        <p className="text-2xl font-semibold tabular-nums">
          {money(s?.amount_total_minor ?? request.amount.minor)}
        </p>
        <p className="text-sm text-muted-foreground">
          {[
            request.requester
              ? t("card.requestedBy", { name: request.requester.name })
              : null,
            request.revision > 1
              ? t("card.revision", { n: request.revision })
              : null,
          ]
            .filter(Boolean)
            .join(" · ")}
        </p>
      </header>

      {request.status === "blocked" && request.blocked_reasons.length ? (
        <Notice variant="warning" title={t("card.blockedTitle")} live={false}>
          <ul className="flex flex-col gap-1">
            {request.blocked_reasons.map((b, i) => {
              const href = mappingHref?.(b)
              return (
                <li key={i} className="flex flex-wrap gap-x-2">
                  <span>{blockedReason(t, b)}</span>
                  {href ? (
                    <a
                      className="font-medium underline underline-offset-4"
                      href={href}
                    >
                      {t("card.fixMapping")}
                    </a>
                  ) : null}
                </li>
              )
            })}
          </ul>
        </Notice>
      ) : null}
      {request.status === "invalidated" ? (
        <Notice variant="info" title={t("card.invalidatedTitle")} live={false}>
          {label(t, "invalidated", request.invalidated_reason)}
        </Notice>
      ) : null}

      {s ? (
        <section
          aria-label={t("card.approval")}
          className="flex flex-col gap-3"
        >
          <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
            <dt className="text-muted-foreground">{t("card.vendor")}</dt>
            <dd>{s.vendor.label ?? t("card.noName")}</dd>
            <dt className="text-muted-foreground">{t("card.company")}</dt>
            <dd>{s.company.label ?? t("card.noName")}</dd>
          </dl>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <caption className="sr-only">{t("card.lines")}</caption>
              <thead className="text-left text-muted-foreground">
                <tr>
                  <th className="py-1 pr-2 font-medium">{t("card.product")}</th>
                  <th className="py-1 pr-2 text-right font-medium">
                    {t("card.quantity")}
                  </th>
                  <th className="py-1 text-right font-medium">
                    {t("card.subtotal")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {s.lines.map((l) => (
                  <tr key={l.source_line_id} className="border-t">
                    <td className="py-1 pr-2">
                      {l.product.label ?? t("card.noName")}
                    </td>
                    <td className="py-1 pr-2 text-right tabular-nums">
                      {trimQty(l.quantity)} {l.uom.label ?? ""}
                    </td>
                    <td className="py-1 text-right tabular-nums">
                      {money(l.subtotal_minor)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <dl className="grid grid-cols-[1fr_auto] gap-x-4 gap-y-1 text-sm tabular-nums">
            <dt>{t("card.untaxed")}</dt>
            <dd className="text-right">{money(s.amount_untaxed_minor)}</dd>
            <dt>{t("card.tax")}</dt>
            <dd className="text-right">{money(s.amount_tax_minor)}</dd>
            <dt className="font-medium">{t("card.total")}</dt>
            <dd className="text-right font-medium">
              {money(s.amount_total_minor)}
            </dd>
          </dl>
          <p className="text-sm text-muted-foreground">{t("card.taxNote")}</p>
        </section>
      ) : null}

      {request.decision ? (
        <p className="text-sm">
          {t("card.decided", {
            decision: t(
              `card.decision${request.decision.decision[0]!.toUpperCase()}${request.decision.decision.slice(1)}`
            ),
            name: request.decision.actor.name,
          })}
          {request.decision.comment ? ` · ${request.decision.comment}` : ""}
        </p>
      ) : null}

      {error ? (
        <div role="alert">
          <Notice variant="error" title={error} live={false} />
        </div>
      ) : null}

      {request.can_decide ? (
        <section className="flex flex-col gap-2" aria-label={t("card.approve")}>
          <Label htmlFor={`${id}-comment`}>{t("card.commentOptional")}</Label>
          <Textarea
            ref={commentRef}
            id={`${id}-comment`}
            value={comment}
            aria-invalid={needComment && !comment.trim() ? true : undefined}
            aria-describedby={needComment ? `${id}-need` : undefined}
            onChange={(e) => setComment(e.target.value)}
          />
          {needComment && !comment.trim() ? (
            <p id={`${id}-need`} className="text-sm text-destructive">
              {t("card.commentNeeded")}
            </p>
          ) : null}
          <div className="flex flex-wrap gap-2">
            <Button
              size="lg"
              className="min-h-11 px-5"
              disabled={busy}
              onClick={() => onApprove?.(comment.trim() || undefined)}
            >
              {t("card.approve")}
            </Button>
            <Button
              size="lg"
              variant="outline"
              className="min-h-11"
              disabled={busy}
              onClick={needs(onReturn)}
            >
              {t("card.return")}
            </Button>
            <Button
              size="lg"
              variant="destructive"
              className="min-h-11"
              disabled={busy}
              onClick={needs(onReject)}
            >
              {t("card.reject")}
            </Button>
          </div>
        </section>
      ) : null}

      <ExecutionStatus
        execution={request.execution}
        onAction={onAction}
        busy={busy}
        t={t}
      />
    </article>
  )
}

/** "Odoo order P00018", from the snapshot name or the subject's last segment. */
export function orderName(r: ApprovalRequestView, t: Translate) {
  const name = r.snapshot?.source.name ?? r.subject_ref.split(":").pop()
  return name ? t("card.order", { name }) : t("card.orderUnnamed")
}

// "2.00000000" → "2": display only; the API's decimal string is never turned into a float.
const trimQty = (q: string) => (q.includes(".") ? q.replace(/\.?0+$/, "") : q)
