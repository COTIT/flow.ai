"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useAccountingJournalsPost,
  useAccountingJournalsPostReview,
  useAccountingJournalsReverse,
  useAccountingJournalsReverseReview,
} from "@genesy/api-client/endpoints/accounting"
import type { JournalResource, ReviewResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatDay } from "@genesy/ui/lib/format"
import { Field } from "@/components/field"

import {
  Shell,
  showMoney,
  useBookToday,
  useAction,
  useReasons,
  type Outcome,
} from "../../shared"
import { journalTitle } from "../title"

/** The server's review of a post or reversal: each line in both currencies, the totals. */
function ReviewFacts({ review }: { review: ReviewResource }) {
  const t = useTranslations()
  const locale = useLocale()
  const fx = review.currency.id !== review.functional_currency.id
  const money = (a: string) => showMoney(a, review.currency, locale)
  const book = (a: string) => showMoney(a, review.functional_currency, locale)
  return (
    <div className="flex flex-col gap-2 text-[13px]">
      <p className="text-muted-foreground">
        {t("accounting.post.dated", {
          date: formatDay(review.accounting_date, locale),
        })}
      </p>
      <ul className="flex flex-col gap-1">
        {review.lines.map((l) => (
          <li
            key={l.line_no}
            className="flex flex-wrap justify-between gap-x-3"
          >
            <span className="font-medium">{l.account_code}</span>
            <span className="tabular-nums">
              {/^0(\.0+)?$/.test(l.debit)
                ? t("accounting.post.credit", { amount: money(l.credit) })
                : t("accounting.post.debit", { amount: money(l.debit) })}
              {fx
                ? ` · ${book(/^0(\.0+)?$/.test(l.functional_debit) ? l.functional_credit : l.functional_debit)}`
                : ""}
            </span>
          </li>
        ))}
      </ul>
      <p className="flex justify-between border-t border-(--hair) pt-2 font-semibold">
        <span>{t("accounting.post.totals")}</span>
        <span className="tabular-nums">{money(review.totals.debit)}</span>
      </p>
      {fx ? (
        <p className="flex justify-between text-muted-foreground">
          <span>
            {t("accounting.post.inBook", {
              currency: review.functional_currency.code,
            })}
          </span>
          <span className="tabular-nums">
            {book(review.totals.functional_debit)}
          </span>
        </p>
      ) : null}
    </div>
  )
}

function Reviewing() {
  const t = useTranslations()
  return (
    <div aria-busy="true" className="flex flex-col gap-2">
      <span className="sr-only">{t("accounting.post.reviewing")}</span>
      <Skeleton className="h-4 w-2/3" />
      <Skeleton className="h-4 w-1/2" />
    </div>
  )
}

/** Post: the server's review (period, functional amounts, totals) first, then its fingerprint back. */
export function PostDialog({
  journal,
  button,
  outcome,
}: {
  journal: JournalResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const review = useAccountingJournalsPostReview()
  const post = useAccountingJournalsPost({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        outcome.done(
          t("done.accounting.posted", { number: r.data.number ?? "" })
        )
      },
    },
  })
  const r = review.data?.data
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        post.reset()
        review.mutate({ workspace: workspaceId, journal: journal.id })
      }}
      button={button}
      trigger={t("accounting.post.action")}
      title={t("accounting.post.title")}
      description={t("accounting.post.text")}
    >
      {review.isPending ? <Reviewing /> : r ? <ReviewFacts review={r} /> : null}
      <ApiError
        error={review.error ?? post.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>
          {t("actions.cancel")}
        </DialogClose>
        <Button
          disabled={!r || post.isPending}
          onClick={() =>
            r &&
            post.mutate({
              workspace: workspaceId,
              journal: journal.id,
              data: {
                client_ref: a.clientRef,
                expected_version: journal.version,
                payload: { review_fingerprint: r.review_fingerprint },
              },
            })
          }
        >
          {t("accounting.post.action")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/** Reverse a posted manual journal: date and reason, the server's review, then the reversal. */
export function ReverseDialog({
  journal,
  button,
  outcome,
}: {
  journal: JournalResource
  button: React.ReactElement
  outcome: Outcome
}) {
  const t = useTranslations()
  const bookToday = useBookToday()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [asked, setAsked] = React.useState({
    accounting_date: "",
    reason: "",
  })
  const review = useAccountingJournalsReverseReview()
  const reverse = useAccountingJournalsReverse({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        outcome.done(
          t("done.accounting.reversed", { title: journalTitle(t, r.data) })
        )
      },
    },
  })
  const r = review.data?.data
  const errors = fieldErrors(review.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        review.reset()
        reverse.reset()
      }}
      button={button}
      trigger={t("accounting.reverse.action")}
      title={t("accounting.reverse.title", { title: journalTitle(t, journal) })}
      description={t("accounting.reverse.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          const body = {
            accounting_date: String(f.get("accounting_date") ?? ""),
            reason: String(f.get("reason") ?? "").trim(),
          }
          setAsked(body)
          reverse.reset()
          review.mutate({
            workspace: workspaceId,
            journal: journal.id,
            data: body,
          })
        }}
      >
        <Field
          id="rv-date"
          label={t("accounting.journals.date")}
          errors={errors.accounting_date}
        >
          <Input
            type="date"
            name="accounting_date"
            required
            key={asked.accounting_date || bookToday}
            defaultValue={asked.accounting_date || bookToday}
            onChange={() => review.reset()}
          />
        </Field>
        <Field
          id="rv-reason"
          label={t("accounting.journals.reason")}
          errors={errors.reason}
        >
          <Textarea
            name="reason"
            required
            maxLength={1000}
            defaultValue={asked.reason}
            onChange={() => review.reset()}
          />
        </Field>
        {review.isPending ? (
          <Reviewing />
        ) : r ? (
          <ReviewFacts review={r} />
        ) : null}
        <ApiError
          error={review.error ?? reverse.error}
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
              variant="destructive"
              disabled={reverse.isPending}
              onClick={() =>
                reverse.mutate({
                  workspace: workspaceId,
                  journal: journal.id,
                  data: {
                    client_ref: a.clientRef,
                    expected_version: journal.version,
                    payload: {
                      ...asked,
                      review_fingerprint: r.review_fingerprint,
                    },
                  },
                })
              }
            >
              {t("accounting.reverse.action")}
            </Button>
          ) : (
            <Button type="submit" disabled={review.isPending}>
              {t("accounting.reverse.review")}
            </Button>
          )}
        </DialogFooter>
      </form>
    </Shell>
  )
}
