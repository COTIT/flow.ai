"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  getAccountingExceptionsGetQueryKey,
  getAccountingExceptionsListQueryKey,
  useAccountingExceptionsGet,
  useAccountingExceptionsResolve,
  useAccountingExceptionsResolveReview,
  useAccountingExceptionsRetry,
} from "@verzo/api-client/endpoints/accounting"
import { useRelationsList } from "@verzo/api-client/endpoints/relations"
import type { ExceptionResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { Card } from "@verzo/ui/components/card"
import { DialogClose, DialogFooter } from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import { Skeleton } from "@verzo/ui/components/skeleton"
import { Textarea } from "@verzo/ui/components/textarea"
import { formatDateTime, formatDay } from "@verzo/ui/lib/format"
import { Bento, DetailSkeleton, SubTile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Capped, Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import {
  AccountingTabs,
  bookAmount,
  Shell,
  useAction,
  useWord,
  type Outcome,
} from "../../shared"
import {
  useExceptionBadge,
  useExceptionReasons,
  useExceptionWord,
  useMinor,
  useWhy,
} from "../shared"

const link =
  "underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"

/** One accounting exception: what the provider reported, why it waits, and the one next step. */
export default function ExceptionPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const rights = useRights()
  const badge = useExceptionBadge()
  const word = useExceptionWord()
  const why = useWhy()
  const minor = useMinor()
  const queryClient = useQueryClient()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const exception = useAccountingExceptionsGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  const done = useDone()
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getAccountingExceptionsGetQueryKey(workspaceId, id),
      getAccountingExceptionsListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const outcome: Outcome = {
    done: (message) => {
      setStale(false)
      done.show(message)
      refresh()
    },
    stale: () => {
      done.clear()
      setStale(true)
      refresh()
    },
  }

  if (exception.isPending) return <DetailSkeleton />
  if (exception.isError)
    return (
      <LoadError
        error={exception.error}
        onRetry={() => void exception.refetch()}
        backHref="/accounting/exceptions"
      />
    )
  const e = exception.data.data
  const mayRetry =
    e.allowed_actions.includes("retry") && rights.may("accounting.post")
  const mayResolve =
    e.allowed_actions.includes("resolve") && rights.may("accounting.post")

  return (
    <>
      <PageHead
        back={{
          href: "/accounting/exceptions",
          label: t("accounting.exceptions.title"),
        }}
        title={t("accounting.exceptions.headline", {
          kind: word("economic", e.economic_kind),
          provider: e.provider,
        })}
        description={why(e)}
        tabs={<AccountingTabs />}
        actions={<StatusBadge status={e.state} {...badge(e.state)} />}
      />
      {stale ? (
        <Notice
          variant="warning"
          title={t("accounting.reasons.stale_document")}
        />
      ) : null}
      <Bento>
        <Panel
          id="ex-next"
          title={t("accounting.exceptions.nextStep")}
          span="col-span-6"
        >
          <Done message={done.message} />
          {e.state === "pending" && e.kind === "pending_posting" ? (
            <Notice
              variant="warning"
              title={t("accounting.exceptions.unposted")}
            />
          ) : null}
          {mayRetry ? (
            <>
              <p className="text-[13px]">
                {t("accounting.exceptions.next.retry")}
              </p>
              <RetryDialog exception={e} outcome={outcome} />
            </>
          ) : null}
          {mayResolve ? (
            <>
              <p className="text-[13px]">
                {t("accounting.exceptions.next.resolve")}
              </p>
              <ResolveDialog exception={e} outcome={outcome} />
            </>
          ) : null}
          {!mayRetry && !mayResolve ? (
            <Notice
              title={
                e.state === "resolved"
                  ? t("accounting.exceptions.resolvedInfo", {
                      how: word("resolutions", e.resolution ?? ""),
                    })
                  : t("accounting.exceptions.nothingToDo")
              }
            />
          ) : null}
          {e.last_error ? (
            <p className="text-[13px] break-words text-muted-foreground">
              {t("accounting.exceptions.lastAttempt", {
                attempts: e.attempts,
                message: e.last_error,
              })}
            </p>
          ) : null}
        </Panel>

        <Panel
          id="ex-facts"
          title={t("accounting.exceptions.details")}
          span="col-span-6"
        >
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact
              label={t("accounting.exceptions.amount")}
              value={minor(e.observed_amount, e)}
            />
            {e.amount != null ? (
              <Fact
                label={t("accounting.exceptions.discrepancy")}
                value={minor(e.amount, e)}
              />
            ) : null}
            {e.recovery_residual != null ? (
              <Fact
                label={t("accounting.exceptions.recoveryLeft")}
                value={minor(e.recovery_residual, e)}
              />
            ) : null}
            <Fact
              label={t("accounting.exceptions.provider")}
              value={e.provider}
            />
            <Fact
              label={t("accounting.exceptions.reference")}
              value={e.external_id}
            />
            <Fact
              label={t("accounting.exceptions.occurred")}
              value={formatDateTime(e.occurred_at, locale)}
            />
            <Fact
              label={t("accounting.exceptions.book")}
              value={word("dispositions", e.disposition ?? "none")}
            />
            {e.resolved_at ? (
              <Fact
                label={t("accounting.exceptions.resolvedAt")}
                value={formatDateTime(e.resolved_at, locale)}
              />
            ) : null}
          </SubTile>
          <nav
            aria-label={t("accounting.exceptions.sources")}
            className="flex flex-col gap-1 text-[13px]"
          >
            {e.invoice_id ? (
              <Link className={link} href={`/invoices/${e.invoice_id}`}>
                {t("accounting.exceptions.openInvoice")}
              </Link>
            ) : null}
            {e.credit_note_id ? (
              <Link className={link} href={`/invoices/${e.credit_note_id}`}>
                {t("accounting.exceptions.openCreditNote")}
              </Link>
            ) : null}
            {e.order_id ? (
              <Link className={link} href={`/orders/${e.order_id}`}>
                {t("accounting.exceptions.openOrder")}
              </Link>
            ) : null}
          </nav>
        </Panel>
      </Bento>
    </>
  )
}

/** Retry: posts the stored provider fact again (no provider call). Still failing: the reason. */
function RetryDialog({
  exception,
  outcome,
}: {
  exception: ExceptionResource
  outcome: Outcome
}) {
  const t = useTranslations()
  const reasons = useExceptionReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const retry = useAccountingExceptionsRetry({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("accounting.exceptions.done.retried"))
      },
    },
  })
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        retry.reset()
      }}
      button={<Button size="lg" className="w-full" />}
      trigger={t("accounting.exceptions.retry.action")}
      title={t("accounting.exceptions.retry.title")}
      description={t("accounting.exceptions.retry.text")}
    >
      <ApiError
        error={retry.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>
          {t("actions.cancel")}
        </DialogClose>
        <Button
          disabled={retry.isPending}
          onClick={() =>
            retry.mutate({
              workspace: workspaceId,
              exception: exception.id,
              data: {
                client_ref: a.clientRef,
                expected_version: exception.version,
                payload: {},
              },
            })
          }
        >
          {t("accounting.exceptions.retry.action")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/**
 * Resolve a discrepancy: assign it to a customer as a receivable or write it off, with reason
 * and evidence; the server's review (amount, date, debit and credit) first, then its fingerprint.
 */
function ResolveDialog({
  exception,
  outcome,
}: {
  exception: ExceptionResource
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useExceptionReasons()
  const role = useWord()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [form, setForm] = React.useState({
    resolution: "assign_receivable",
    relation_id: exception.relation_id ?? "",
    reason: "",
    evidence_ref: "",
  })
  const assign = form.resolution === "assign_receivable"
  const relations = useRelationsList(
    workspaceId,
    { limit: 100 },
    { query: { enabled: Boolean(workspaceId) && a.open && assign } }
  )
  const review = useAccountingExceptionsResolveReview()
  const resolve = useAccountingExceptionsResolve({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("accounting.exceptions.done.resolved"))
      },
    },
  })
  const body = {
    resolution: form.resolution,
    relation_id: assign && form.relation_id ? form.relation_id : null,
    reason: form.reason.trim(),
    evidence_ref: form.evidence_ref.trim(),
  }
  const set = (patch: Partial<typeof form>) => {
    setForm((f) => ({ ...f, ...patch }))
    review.reset()
  }
  const r = review.data?.data
  const errors = fieldErrors(review.error ?? resolve.error)
  const err = (k: string) => errors[k] ?? errors[`payload.${k}`]
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        review.reset()
        resolve.reset()
      }}
      button={<Button size="lg" className="w-full" />}
      trigger={t("accounting.exceptions.resolve.action")}
      title={t("accounting.exceptions.resolve.title")}
      description={t("accounting.exceptions.resolve.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(ev) => {
          ev.preventDefault()
          resolve.reset()
          review.mutate({
            workspace: workspaceId,
            exception: exception.id,
            data: body,
          })
        }}
      >
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-1 text-sm font-medium">
            {t("accounting.exceptions.resolve.how")}
          </legend>
          {["assign_receivable", "write_off"].map((v) => (
            <label
              key={v}
              className="flex min-h-11 items-center gap-2 text-sm md:min-h-0"
            >
              <input
                type="radio"
                name="resolution"
                value={v}
                checked={form.resolution === v}
                onChange={() => set({ resolution: v })}
              />
              {t(`accounting.exceptions.resolutions.${v}`)}
            </label>
          ))}
        </fieldset>
        {assign ? (
          <>
            <Capped meta={relations.data?.meta} />
            <Field
              id="ex-relation"
              label={t("accounting.exceptions.resolve.customer")}
              errors={err("relation_id")}
            >
              <select
                className={selectClass}
                value={form.relation_id}
                onChange={(ev) => set({ relation_id: ev.target.value })}
              >
                <option value="">
                  {t("accounting.exceptions.resolve.defaultCustomer")}
                </option>
                {relations.data?.data.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.company_name ?? c.alias ?? c.relation_nr ?? c.id}
                  </option>
                ))}
              </select>
            </Field>
          </>
        ) : null}
        <Field
          id="ex-reason"
          label={t("accounting.exceptions.resolve.reason")}
          errors={err("reason")}
        >
          <Textarea
            required
            maxLength={1000}
            value={form.reason}
            onChange={(ev) => set({ reason: ev.target.value })}
          />
        </Field>
        <Field
          id="ex-evidence"
          label={t("accounting.exceptions.resolve.evidence")}
          errors={err("evidence_ref")}
        >
          <Input
            required
            maxLength={255}
            value={form.evidence_ref}
            onChange={(ev) => set({ evidence_ref: ev.target.value })}
          />
        </Field>
        {review.isPending ? (
          <div aria-busy="true" className="flex flex-col gap-2">
            <span className="sr-only">
              {t("accounting.exceptions.reviewing")}
            </span>
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        ) : r ? (
          <div className="flex flex-col gap-1 text-[13px]">
            <Fact
              label={t("accounting.exceptions.amount")}
              value={bookAmount(r.amount, locale)}
            />
            <Fact
              label={t("accounting.exceptions.resolve.date")}
              value={formatDay(r.accounting_date, locale)}
            />
            <Fact
              label={t("accounting.exceptions.resolve.debit")}
              value={role("roles", r.debit)}
            />
            <Fact
              label={t("accounting.exceptions.resolve.credit")}
              value={role("roles", r.credit)}
            />
          </div>
        ) : null}
        <ApiError
          error={review.error ?? resolve.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          {r ? (
            <Button
              type="button"
              disabled={resolve.isPending}
              onClick={() =>
                resolve.mutate({
                  workspace: workspaceId,
                  exception: exception.id,
                  data: {
                    client_ref: a.clientRef,
                    expected_version: exception.version,
                    payload: {
                      ...body,
                      review_fingerprint: r.review_fingerprint,
                    },
                  },
                })
              }
            >
              {t("accounting.exceptions.resolve.action")}
            </Button>
          ) : (
            <Button type="submit" disabled={review.isPending}>
              {t("accounting.exceptions.review")}
            </Button>
          )}
        </DialogFooter>
      </form>
    </Shell>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <p className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="text-right break-words">{value}</span>
    </p>
  )
}

function Panel({
  id,
  title,
  span,
  children,
}: {
  id: string
  title: string
  span: string
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className={`${span} gap-4 px-6 py-6 max-md:col-span-12 max-md:gap-3 max-md:p-4`}
    >
      <h2
        id={id}
        className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
      >
        {title}
      </h2>
      {children}
    </Card>
  )
}
