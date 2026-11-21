"use client"

import * as React from "react"
import { ArrowLeft } from "lucide-react"

import { Badge } from "../badge"
import { Button } from "../button"
import { EmptyState } from "../empty-state"
import { Label } from "../label"
import { Notice } from "../notice"
import { Skeleton } from "../skeleton"
import { Textarea } from "../textarea"
import { CODES, errorText, label } from "./labels"
import type { IncidentDetailView, IncidentView, Translate } from "./types"

type Query<T> = {
  data?: T
  isPending: boolean
  isError: boolean
  error: unknown
  refetch(): unknown
}

/**
 * Sync problems (WF-03) for Office: open/closed and category filters, list beside detail on wide
 * screens, one at a time on a phone. Display only; the API decides every state and action.
 * Shows the readable sentence, never a provider code.
 */
export function IncidentsScreen({
  s,
  t,
  locale,
  runHref,
}: {
  s: {
    status: "open" | "resolved"
    setStatus(s: "open" | "resolved"): void
    category: string | null
    setCategory(c: string | null): void
    list: Query<{ data: IncidentView[] }>
    selectedId: string | null
    select(id: string | null): void
    detail: Query<{ data: IncidentDetailView }>
    incident?: IncidentDetailView
    busy: boolean
    error: unknown
    done: "verified" | "resolved" | null
    verify(): void
    resolve(note: string): void
  }
  t: Translate
  locale: string
  runHref?: (i: IncidentDetailView) => string | undefined
}) {
  const items = s.list.data?.data ?? []
  const when = (iso: string) =>
    new Intl.DateTimeFormat(locale, {
      dateStyle: "medium",
      timeStyle: "short",
    }).format(new Date(iso))

  return (
    <div className="grid gap-4 md:grid-cols-[minmax(16rem,22rem)_1fr]">
      <section
        aria-label={t("incidents.title")}
        className={`flex min-w-0 flex-col gap-3 ${s.selectedId ? "max-md:hidden" : ""}`}
      >
        <div
          role="group"
          aria-label={t("incidents.title")}
          className="flex gap-2"
        >
          {(["open", "resolved"] as const).map((st) => (
            <Button
              key={st}
              size="lg"
              className="min-h-11"
              variant={s.status === st ? "secondary" : "ghost"}
              aria-pressed={s.status === st}
              onClick={() => s.setStatus(st)}
            >
              {t(
                st === "open"
                  ? "incidents.statusOpen"
                  : "incidents.statusResolved"
              )}
            </Button>
          ))}
        </div>
        <div
          role="group"
          aria-label={t("incidents.filter")}
          className="flex flex-wrap gap-2"
        >
          {[null, ...CODES.category].map((c) => (
            <Button
              key={c ?? "all"}
              size="sm"
              className="min-h-11 min-w-11"
              variant={s.category === c ? "secondary" : "outline"}
              aria-pressed={s.category === c}
              onClick={() => s.setCategory(c)}
            >
              {c ? label(t, "category", c) : t("incidents.all")}
            </Button>
          ))}
        </div>
        {s.list.isPending ? (
          <div className="flex flex-col gap-2">
            <Skeleton className="h-16" />
            <Skeleton className="h-16" />
          </div>
        ) : s.list.isError ? (
          <Notice
            variant="error"
            title={t("incidents.loadError")}
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
        ) : items.length === 0 ? (
          <EmptyState
            title={t(
              s.category ? "incidents.emptyFiltered" : "incidents.empty"
            )}
          />
        ) : (
          <ul className="flex flex-col gap-2" data-testid="incident-list">
            {items.map((i) => (
              <li key={i.id}>
                <button
                  type="button"
                  aria-current={s.selectedId === i.id ? "true" : undefined}
                  onClick={() => s.select(i.id)}
                  className="flex w-full min-w-0 flex-col items-start gap-1 rounded-md border p-3 text-left hover:bg-muted aria-[current=true]:border-primary"
                >
                  <span className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">
                      {label(t, "incident", i.kind)}
                    </span>
                    {i.category ? (
                      <Badge variant="slate">
                        {label(t, "category", i.category)}
                      </Badge>
                    ) : null}
                  </span>
                  {i.diagnosis ? (
                    <span className="line-clamp-2 text-sm text-muted-foreground">
                      {i.diagnosis.sentence}
                    </span>
                  ) : null}
                  <span className="text-xs text-muted-foreground">
                    {t("incidents.raised", { date: when(i.created_at) })}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section
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
            {t("incidents.back")}
          </Button>
        ) : null}
        {!s.selectedId ? (
          <EmptyState title={t("incidents.choose")} />
        ) : s.detail.isPending ? (
          <Skeleton className="h-64" />
        ) : s.detail.isError ? (
          <Notice variant="error" title={errorText(t, s.detail.error)} />
        ) : s.incident ? (
          <Detail
            i={s.incident}
            s={s}
            t={t}
            when={when}
            runHref={runHref?.(s.incident)}
          />
        ) : null}
      </section>
    </div>
  )
}

/** The Odoo order number in a `odoo:<connection>:purchase.order:<id>` reference, for display. */
function order(subject: string) {
  return /:purchase\.order:(\d+)$/.exec(subject)?.[1]
}

function Detail({
  i,
  s,
  t,
  when,
  runHref,
}: {
  i: IncidentDetailView
  s: {
    busy: boolean
    error: unknown
    done: "verified" | "resolved" | null
    verify(): void
    resolve(note: string): void
  }
  t: Translate
  when(iso: string): string
  runHref?: string
}) {
  const id = React.useId()
  const [closing, setClosing] = React.useState(false)
  const open = i.status === "open"
  const exp = i.expectation
  const canVerify =
    open && (exp?.status === "pending" || exp?.status === "late")
  const done =
    s.done === "verified"
      ? open
        ? { variant: "info" as const, text: t("incidents.checkedMissing") }
        : { variant: "success" as const, text: t("incidents.checked") }
      : s.done === "resolved"
        ? { variant: "success" as const, text: t("incidents.closedManual") }
        : null
  return (
    <article className="flex flex-col gap-4" data-testid="incident-detail">
      <header className="flex flex-wrap items-center gap-2">
        <h2 className="text-lg font-semibold">
          {label(t, "incident", i.kind)}
        </h2>
        <Badge variant={open ? "red" : "slate"}>
          {t(open ? "incidents.open" : "incidents.resolved")}
        </Badge>
        {i.category ? (
          <Badge variant="slate">{label(t, "category", i.category)}</Badge>
        ) : null}
      </header>
      <div role="status">
        {done ? (
          <Notice variant={done.variant} live={false} title={done.text} />
        ) : null}
      </div>
      {s.error ? (
        <Notice variant="error" title={errorText(t, s.error)} />
      ) : null}
      {!open ? (
        <Notice
          variant="success"
          title={t(
            i.resolution_kind === "reconciled"
              ? "incidents.closedReconciled"
              : "incidents.closedManual"
          )}
        >
          {i.note ?? null}
        </Notice>
      ) : null}

      <section className="flex flex-col gap-1">
        <h3 className="text-sm font-medium">{t("incidents.whatHappened")}</h3>
        <p className="text-sm">
          {i.diagnosis?.sentence ?? label(t, "incident", i.kind)}
        </p>
        {order(i.subject_ref) ? (
          <p className="text-sm text-muted-foreground">
            {t("incidents.records")}:{" "}
            {t("card.order", { name: order(i.subject_ref)! })}
          </p>
        ) : null}
      </section>

      {exp ? (
        <section className="flex flex-col gap-1">
          <h3 className="text-sm font-medium">{t("incidents.watch")}</h3>
          <p className="text-sm">
            {t(`incidents.exp_${exp.status}`)} · {t("incidents.due")}{" "}
            {when(exp.due_at)}
          </p>
          <p className="text-sm text-muted-foreground">
            {t("incidents.checks", { count: exp.checks })} ·{" "}
            {exp.last_checked_at
              ? t("incidents.lastChecked", { date: when(exp.last_checked_at) })
              : t("incidents.noCheck")}
          </p>
        </section>
      ) : null}

      <section className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">{t("incidents.timeline")}</h3>
        {i.timeline.length ? (
          <ol className="flex flex-col gap-1 text-sm">
            {i.timeline.map((e, n) => (
              <li key={n} className="flex flex-wrap gap-x-2">
                <span className="text-muted-foreground">
                  {when(e.created_at)}
                </span>
                <span>{e.summary}</span>
              </li>
            ))}
          </ol>
        ) : (
          <p className="text-sm text-muted-foreground">
            {t("incidents.noTimeline")}
          </p>
        )}
      </section>

      {open ? (
        <div className="flex flex-wrap gap-2">
          {canVerify ? (
            <Button size="lg" disabled={s.busy} onClick={s.verify}>
              {t("incidents.check")}
            </Button>
          ) : null}
          {!closing ? (
            <Button
              size="lg"
              variant="outline"
              disabled={s.busy}
              onClick={() => setClosing(true)}
            >
              {t("incidents.resolve")}
            </Button>
          ) : null}
          {runHref ? (
            <a
              href={runHref}
              className="inline-flex min-h-11 items-center px-2 text-sm font-medium underline underline-offset-4"
            >
              {t("incidents.openRun")}
            </a>
          ) : null}
        </div>
      ) : null}
      {open && closing ? (
        <form
          className="flex flex-col gap-2"
          aria-label={t("incidents.resolveTitle")}
          onSubmit={(e) => {
            e.preventDefault()
            const note = String(
              new FormData(e.currentTarget).get("note") ?? ""
            ).trim()
            if (note) s.resolve(note)
          }}
        >
          <Label htmlFor={`${id}-note`}>{t("incidents.note")}</Label>
          <Textarea
            id={`${id}-note`}
            name="note"
            required
            aria-describedby={`${id}-hint`}
          />
          <p id={`${id}-hint`} className="text-sm text-muted-foreground">
            {t("incidents.noteNeeded")}
          </p>
          <div className="flex flex-wrap gap-2">
            <Button type="submit" size="lg" disabled={s.busy}>
              {t("incidents.confirm")}
            </Button>
            <Button
              type="button"
              size="lg"
              variant="ghost"
              onClick={() => setClosing(false)}
            >
              {t("incidents.cancel")}
            </Button>
          </div>
        </form>
      ) : null}
      <p className="text-xs text-muted-foreground">{t("incidents.contract")}</p>
    </article>
  )
}
