"use client"

import * as React from "react"

import { Badge } from "../badge"
import { Button } from "../button"
import { Input } from "../input"
import { Label } from "../label"
import { Textarea } from "../textarea"
import { label } from "./labels"
import type { ExecutionView, ResolveAction, Translate } from "./types"

type Tone = "teal" | "gold" | "red" | "slate"
const tones: Record<string, Tone> = {
  succeeded: "teal",
  queued: "slate",
  running: "slate",
  awaiting_reconciliation: "gold",
  succeeded_with_variance: "gold",
  blocked: "red",
  failed: "red",
  cancelled: "slate",
  resolved_manual: "slate",
}

/**
 * The NetSuite side of a request, apart from its approval status (PLAN test 23).
 * Actions come only from the API's `allowed_actions`; the API still checks every call.
 * "Verified" shows only when the API says `verified`.
 */
export function ExecutionStatus({
  execution,
  onAction,
  busy,
  t,
}: {
  execution: ExecutionView | null
  onAction?(
    action: ResolveAction | "retry",
    reason: string,
    targetId?: string
  ): void
  busy?: boolean
  t: Translate
}) {
  const [action, setAction] = React.useState<ResolveAction | "retry" | null>(
    null
  )
  const id = React.useId()
  if (!execution)
    return (
      <section
        aria-label={t("execution.title")}
        className="flex flex-col gap-2"
      >
        <h3 className="text-sm font-medium">{t("execution.title")}</h3>
        <p className="text-sm text-muted-foreground">{t("execution.none")}</p>
      </section>
    )
  const why =
    execution.status === "resolved_manual"
      ? label(t, "resolution", execution.resolution)
      : execution.reason
        ? label(t, "executionReason", execution.reason)
        : null
  const actions = onAction ? (execution.allowed_actions ?? []) : []
  return (
    <section
      aria-label={t("execution.title")}
      data-testid="execution-status"
      className="flex flex-col gap-2"
    >
      <h3 className="text-sm font-medium">{t("execution.title")}</h3>
      <div className="flex flex-wrap items-center gap-2">
        <Badge variant={tones[execution.status] ?? "slate"}>
          {label(t, "executionStatus", execution.status)}
        </Badge>
        <span className="text-sm text-muted-foreground">
          {execution.verified
            ? t("execution.verified")
            : t("execution.notVerified")}
        </span>
      </div>
      {execution.status !== "succeeded" ? (
        <p className="text-sm text-muted-foreground">
          {t("execution.step", { phase: label(t, "phase", execution.phase) })}
        </p>
      ) : null}
      {why ? <p className="text-sm">{why}</p> : null}
      {execution.error_message ? (
        <p className="text-sm text-muted-foreground">
          {execution.error_message}
        </p>
      ) : null}
      {execution.target_ref ? (
        <p className="text-sm">
          {t("execution.target", {
            ref: execution.target_ref.split(":").pop() ?? "",
          })}
        </p>
      ) : null}
      {actions.length ? (
        <div className="flex flex-col gap-2">
          <p className="text-sm font-medium">{t("execution.actions")}</p>
          <div className="flex flex-wrap gap-2">
            {actions.map((a) => (
              <Button
                key={a}
                size="lg"
                variant={a === action ? "secondary" : "outline"}
                aria-pressed={a === action}
                disabled={busy}
                onClick={() => setAction(a)}
              >
                {label(t, "action", a)}
              </Button>
            ))}
          </div>
          {action ? (
            <form
              className="flex flex-col gap-2"
              onSubmit={(e) => {
                e.preventDefault()
                const f = new FormData(e.currentTarget)
                onAction?.(
                  action,
                  String(f.get("reason") ?? "").trim(),
                  action === "link_existing"
                    ? String(f.get("target_id") ?? "").trim()
                    : undefined
                )
              }}
            >
              {action === "link_existing" ? (
                <>
                  <Label htmlFor={`${id}-target`}>
                    {t("execution.targetId")}
                  </Label>
                  <Input id={`${id}-target`} name="target_id" required />
                </>
              ) : null}
              <Label htmlFor={`${id}-reason`}>{t("execution.reason")}</Label>
              <Textarea
                id={`${id}-reason`}
                name="reason"
                required
                aria-describedby={`${id}-hint`}
              />
              <p id={`${id}-hint`} className="text-sm text-muted-foreground">
                {t("execution.reasonHint")}
              </p>
              <div className="flex flex-wrap gap-2">
                <Button type="submit" size="lg" disabled={busy}>
                  {label(t, "action", action)}
                </Button>
                <Button
                  type="button"
                  size="lg"
                  variant="ghost"
                  onClick={() => setAction(null)}
                >
                  {t("execution.cancel")}
                </Button>
              </div>
            </form>
          ) : null}
        </div>
      ) : null}
    </section>
  )
}
