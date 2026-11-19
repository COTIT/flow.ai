"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useStocktakesCancel,
  useStocktakesPost,
  useStocktakesPostReview,
  useStocktakesStart,
} from "@genesy/api-client/endpoints/stocktakes"
import type { StocktakeResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Skeleton } from "@genesy/ui/components/skeleton"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatQty } from "@genesy/ui/lib/format"
import { Field } from "@/components/field"

import {
  Shell,
  today,
  useAction,
  useCountReasons,
  type Outcome,
} from "../shared"

type Props = {
  stocktake: StocktakeResource
  outcome: Outcome
  button: React.ReactElement
}

/** A line's label: product, SKU, bin, lot, from the API's display fields. */
export function lineLabel(l: NonNullable<StocktakeResource["lines"]>[number]) {
  return [l.product_name, l.product_reference, l.bin_code, l.lot_code]
    .filter(Boolean)
    .join(" · ")
}

/** Start: freezes the warehouse's stock and pauses it until the count ends. */
export function StartDialog({ stocktake, outcome, button }: Props) {
  const t = useTranslations()
  const reasons = useCountReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const start = useStocktakesStart({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.stocktakes.started"))
      },
    },
  })
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        start.reset()
      }}
      button={button}
      trigger={t("stocktakes.start.action")}
      title={t("stocktakes.start.title")}
      description={t("stocktakes.start.text")}
    >
      <ApiError
        error={start.error}
        fallback={t("common.error")}
        codes={reasons}
      />
      <DialogFooter>
        <DialogClose render={<Button type="button" variant="outline" />}>
          {t("actions.cancel")}
        </DialogClose>
        <Button
          disabled={start.isPending}
          onClick={() =>
            start.mutate({
              workspace: workspaceId,
              stocktake: stocktake.id,
              data: {
                client_ref: a.clientRef,
                expected_version: stocktake.version,
                payload: {},
              },
            })
          }
        >
          {t("stocktakes.start.action")}
        </Button>
      </DialogFooter>
    </Shell>
  )
}

/** Cancel: keeps the lines as history, moves no stock, resumes the warehouse. */
export function CancelDialog({ stocktake, outcome, button }: Props) {
  const t = useTranslations()
  const reasons = useCountReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const cancel = useStocktakesCancel({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.stocktakes.cancelled"))
      },
    },
  })
  const errors = fieldErrors(cancel.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        cancel.reset()
      }}
      button={button}
      trigger={t("stocktakes.cancel.action")}
      title={t("stocktakes.cancel.title")}
      description={t("stocktakes.cancel.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          cancel.mutate({
            workspace: workspaceId,
            stocktake: stocktake.id,
            data: {
              client_ref: a.clientRef,
              expected_version: stocktake.version,
              payload: { reason: String(f.get("reason") ?? "").trim() },
            },
          })
        }}
      >
        <Field
          id="st-cancel-reason"
          label={t("stocktakes.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError
          error={cancel.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("stocktakes.cancel.keep")}
          </DialogClose>
          <Button
            type="submit"
            variant="destructive"
            disabled={cancel.isPending}
          >
            {t("stocktakes.cancel.action")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Post: a date, the server's review (difference and value per changed line), then its fingerprint back. */
export function PostDialog({ stocktake, outcome, button }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useCountReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [date, setDate] = React.useState(today())
  const review = useStocktakesPostReview()
  const post = useStocktakesPost({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.stocktakes.posted"))
      },
    },
  })
  const r = review.data?.data
  const errors = fieldErrors(review.error)
  const label = (id: string) => {
    const l = stocktake.lines?.find((x) => x.stock_identity === id)
    return l ? lineLabel(l) : ""
  }
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        review.reset()
        post.reset()
      }}
      button={button}
      trigger={t("stocktakes.post.action")}
      title={t("stocktakes.post.title")}
      description={t("stocktakes.post.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          post.reset()
          review.mutate({
            workspace: workspaceId,
            stocktake: stocktake.id,
            data: { accounting_date: date },
          })
        }}
      >
        <Field
          id="st-post-date"
          label={t("stocktakes.post.date")}
          errors={errors.accounting_date}
        >
          <Input
            type="date"
            required
            value={date}
            onChange={(e) => {
              setDate(e.target.value)
              review.reset()
            }}
          />
        </Field>
        {review.isPending ? (
          <div aria-busy="true" className="flex flex-col gap-2">
            <span className="sr-only">{t("stocktakes.post.reviewing")}</span>
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        ) : r ? (
          <div className="flex flex-col gap-2 text-[13px]">
            {r.lines.length === 0 ? (
              <p>{t("stocktakes.post.noChanges")}</p>
            ) : (
              <ul className="flex flex-col gap-1">
                {r.lines.map((l) => (
                  <li
                    key={l.stock_identity}
                    className="flex flex-wrap justify-between gap-x-3"
                  >
                    <span className="font-medium">
                      {label(l.stock_identity)}
                    </span>
                    <span className="tabular-nums">
                      {t("stocktakes.post.delta", {
                        delta: formatQty(l.delta, locale),
                      })}
                      {l.value != null
                        ? ` · ${t("stocktakes.post.value", { value: l.value })}`
                        : ""}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : null}
        <ApiError
          error={review.error ?? post.error}
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
              disabled={post.isPending}
              onClick={() =>
                post.mutate({
                  workspace: workspaceId,
                  stocktake: stocktake.id,
                  data: {
                    client_ref: a.clientRef,
                    expected_version: stocktake.version,
                    payload: {
                      accounting_date: r.accounting_date,
                      review_fingerprint: r.review_fingerprint,
                    },
                  },
                })
              }
            >
              {t("stocktakes.post.action")}
            </Button>
          ) : (
            <Button type="submit" disabled={review.isPending}>
              {t("stocktakes.post.review")}
            </Button>
          )}
        </DialogFooter>
      </form>
    </Shell>
  )
}
