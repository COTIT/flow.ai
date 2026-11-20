"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  getAccountingCashPlacesListQueryKey,
  useAccountingCashAdjustmentsCreate,
  useAccountingCashAdjustmentsCreateReview,
  useAccountingCashPlacesList,
} from "@verzo/api-client/endpoints/accounting"
import type { CashPlaceResource } from "@verzo/api-client/model"
import { ApiError, fieldErrors } from "@verzo/ui/components/api-error"
import { Button } from "@verzo/ui/components/button"
import { DialogClose, DialogFooter } from "@verzo/ui/components/dialog"
import { Input } from "@verzo/ui/components/input"
import { Notice } from "@verzo/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { Textarea } from "@verzo/ui/components/textarea"
import { formatMoney } from "@verzo/ui/lib/money"
import { Field } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

import {
  AccountingTabs,
  Shell,
  useBookToday,
  useAction,
  useReasons,
  useWord,
  type Outcome,
} from "../shared"

/** Minor units from the API in the place's own currency, formatted (no maths). */
const useCash = () => {
  const locale = useLocale()
  return (minor: number, c: { code: string; decimals: number }) =>
    formatMoney(minor, c.decimals, c.code, locale)
}

/**
 * Cash places (Template List): each drawer and vault with its book cash. "Count cash" records
 * a count that differs from the book through the server's review, then posts the adjustment.
 */
export default function CashPage() {
  const t = useTranslations()
  const word = useWord()
  const cash = useCash()
  const queryClient = useQueryClient()
  const mayAdjust = useCan("cashregister.adjust")
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const places = useAccountingCashPlacesList(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const done = useDone()
  const refresh = () =>
    void queryClient.invalidateQueries({
      queryKey: getAccountingCashPlacesListQueryKey(workspaceId),
    })
  const outcome: Outcome = {
    done: (message) => {
      done.show(message)
      refresh()
    },
    stale: refresh,
  }
  const columns = [
    { label: t("accounting.cash.place") },
    { label: t("accounting.cash.kind") },
    { label: t("accounting.cash.shift") },
    { label: t("accounting.cash.balance"), right: true },
    { label: t("accounting.cash.actions"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.cash.title")}
        description={t("accounting.cash.hint")}
        tabs={<AccountingTabs />}
      />
      <Done message={done.message} />
      <ListCard>
        {places.isPending ? (
          <ListSkeleton columns={columns} rows={3} />
        ) : places.isError ? (
          <CardState>
            <LoadError
              error={places.error}
              onRetry={() => void places.refetch()}
            />
          </CardState>
        ) : places.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("accounting.cash.empty")} />
          </CardState>
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                {columns.map((c) => (
                  <TableHead
                    key={c.label}
                    className={c.right ? "text-right" : undefined}
                  >
                    {c.label}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {places.data.data.map((p) => (
                <TableRow key={`${p.kind}-${p.id}`}>
                  <TableCell className="font-semibold">{p.name}</TableCell>
                  <TableCell data-m="sub">
                    {word("placeKinds", p.kind)}
                  </TableCell>
                  <TableCell data-m="badge">
                    {p.kind === "drawer" ? (
                      <StatusBadge
                        status={p.open_shift_id ? "open" : "closed"}
                        label={t(
                          p.open_shift_id
                            ? "accounting.cash.shiftOpen"
                            : "accounting.cash.shiftClosed"
                        )}
                        tone={p.open_shift_id ? "teal" : "slate"}
                      />
                    ) : null}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {cash(p.balance, p.currency)}
                  </TableCell>
                  <TableCell className="text-right">
                    {mayAdjust ? (
                      <CountDialog
                        place={p}
                        outcome={outcome}
                        button={<Button variant="outline" size="sm" />}
                      />
                    ) : null}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </ListCard>
    </>
  )
}

/** Count: what is there and the date → the server's expected/difference → evidence and reason → post. */
function CountDialog({
  place,
  outcome,
  button,
}: {
  place: CashPlaceResource
  outcome: Outcome
  button: React.ReactElement
}) {
  const t = useTranslations()
  const bookToday = useBookToday()
  const word = useWord()
  const cash = useCash()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [asked, setAsked] = React.useState({
    counted_amount: "",
    accounting_date: "",
  })
  const review = useAccountingCashAdjustmentsCreateReview()
  const post = useAccountingCashAdjustmentsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: (r) => {
        a.setOpen(false)
        outcome.done(
          t("done.accounting.cashAdjusted", {
            place: place.name,
            difference: cash(r.data.difference, r.data.currency),
          })
        )
      },
    },
  })
  const r = review.data?.data
  const reviewErrors = fieldErrors(review.error)
  const postErrors = fieldErrors(post.error)
  const cashPlace = { kind: place.kind, id: place.id }
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
      trigger={t("accounting.cash.count")}
      title={t("accounting.cash.countTitle", { place: place.name })}
      description={t("accounting.cash.countText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          if (!r) {
            const body = {
              counted_amount: String(f.get("counted_amount") ?? "").trim(),
              accounting_date: String(f.get("accounting_date") ?? ""),
            }
            setAsked(body)
            review.mutate({
              workspace: workspaceId,
              data: { cash_place: cashPlace, ...body },
            })
            return
          }
          post.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: {
                cash_place: cashPlace,
                ...asked,
                count_evidence_ref: String(
                  f.get("count_evidence_ref") ?? ""
                ).trim(),
                reason: String(f.get("reason") ?? "").trim(),
                review_fingerprint: r.review_fingerprint,
              },
            },
          })
        }}
      >
        <Field
          id="cc-counted"
          label={t("accounting.cash.counted", {
            currency: place.currency.code,
          })}
          errors={
            reviewErrors.counted_amount ?? postErrors["payload.counted_amount"]
          }
        >
          <Input
            name="counted_amount"
            inputMode="decimal"
            required
            defaultValue={asked.counted_amount}
            onChange={() => review.reset()}
          />
        </Field>
        <Field
          id="cc-date"
          label={t("accounting.journals.date")}
          errors={
            reviewErrors.accounting_date ??
            postErrors["payload.accounting_date"]
          }
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
        {r ? (
          <>
            <dl className="grid grid-cols-3 gap-3 text-[13px]">
              <div>
                <dt className="text-muted-foreground">
                  {t("accounting.cash.expected")}
                </dt>
                <dd className="font-semibold tabular-nums">
                  {cash(r.expected, r.currency)}
                </dd>
              </div>
              <div>
                <dt className="text-muted-foreground">
                  {t("accounting.cash.countedShort")}
                </dt>
                <dd className="font-semibold tabular-nums">
                  {cash(r.counted, r.currency)}
                </dd>
              </div>
              <div>
                <dt className="text-muted-foreground">
                  {t("accounting.cash.difference")}
                </dt>
                <dd className="font-semibold tabular-nums">
                  {cash(r.difference, r.currency)}
                </dd>
              </div>
            </dl>
            {r.journal ? (
              <Notice
                variant="warning"
                live={false}
                title={t("accounting.cash.journal", {
                  debit: word("roles", r.journal.debit_role),
                  credit: word("roles", r.journal.credit_role),
                  amount: cash(r.journal.amount, r.currency),
                })}
              />
            ) : (
              <Notice live={false} title={t("accounting.cash.noJournal")} />
            )}
            <Field
              id="cc-evidence"
              label={t("accounting.cash.evidence")}
              errors={postErrors["payload.count_evidence_ref"]}
            >
              <Input name="count_evidence_ref" required maxLength={255} />
            </Field>
            <Field
              id="cc-reason"
              label={t("accounting.journals.reason")}
              errors={postErrors["payload.reason"]}
            >
              <Textarea name="reason" required maxLength={1000} />
            </Field>
          </>
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
          <Button type="submit" disabled={review.isPending || post.isPending}>
            {r ? t("accounting.cash.post") : t("accounting.cash.review")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}
