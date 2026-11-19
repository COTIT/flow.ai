"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useAccountingBankAccountsList } from "@genesy/api-client/endpoints/accounting"
import {
  useBankReconciliationsCreate,
  useBankReconciliationsReverse,
} from "@genesy/api-client/endpoints/bank-reconciliations"
import {
  getBankStatementsGetQueryKey,
  getBankStatementsLinesQueryKey,
  getBankStatementsListQueryKey,
  useBankStatementsGet,
  useBankStatementsLineCandidates,
  useBankStatementsLines,
} from "@genesy/api-client/endpoints/bank-statements"
import type { Meta } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import { Notice } from "@genesy/ui/components/notice"
import { Skeleton } from "@genesy/ui/components/skeleton"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { Textarea } from "@genesy/ui/components/textarea"
import { formatDay } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { Field } from "@/components/field"
import { ListPager, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import {
  AccountingTabs,
  bookAmount,
  Shell,
  useAction,
  type Outcome,
} from "../../shared"
import {
  useBankBadge,
  useBankReasons,
  type Candidate,
  type Statement,
  type StatementLine,
} from "../shared"

const PAGE = 50
const link =
  "underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"

/** One bank statement: its balances, then each line with its match and the one next step. */
export default function StatementPage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const rights = useRights()
  const badge = useBankBadge()
  const queryClient = useQueryClient()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const [page, setPage] = React.useState(1)
  const statement = useBankStatementsGet(workspaceId, id, enabled)
  const lines = useBankStatementsLines(
    workspaceId,
    id,
    { page: String(page), page_size: String(PAGE) },
    enabled
  )
  const banks = useAccountingBankAccountsList(workspaceId, enabled)
  const done = useDone()
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getBankStatementsGetQueryKey(workspaceId, id),
      getBankStatementsLinesQueryKey(workspaceId, id),
      getBankStatementsListQueryKey(workspaceId),
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

  if (statement.isPending) return <DetailSkeleton />
  if (statement.isError)
    return (
      <LoadError
        error={statement.error}
        onRetry={() => void statement.refetch()}
        backHref="/accounting/banking"
      />
    )
  const s = statement.data.data as Statement
  const bank =
    banks.data?.data.find((b) => b.id === s.bank_account_id)?.name ?? ""
  const mayMatch = rights.may("accounting.reconcile")
  const rows = (lines.data?.data ?? []) as StatementLine[]

  return (
    <>
      <PageHead
        back={{
          href: "/accounting/banking",
          label: t("accounting.banking.title"),
        }}
        title={s.reference}
        description={t("accounting.banking.of", {
          bank,
          from: formatDay(s.start_date, locale),
          to: formatDay(s.end_date, locale),
        })}
        tabs={<AccountingTabs />}
        actions={<StatusBadge status={s.state} {...badge(s.state)} />}
      />
      {stale ? (
        <Notice
          variant="warning"
          title={t("accounting.reasons.stale_document")}
        />
      ) : null}
      <Done message={done.message} />
      <Bento>
        <Card
          role="region"
          aria-labelledby="bs-facts"
          className="col-span-12 gap-4 px-6 py-6 max-md:gap-3 max-md:p-4"
        >
          <h2
            id="bs-facts"
            className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
          >
            {t("accounting.banking.details")}
          </h2>
          <SubTile className="grid gap-x-8 gap-y-1 text-[13px] md:grid-cols-2">
            <Fact
              label={t("accounting.banking.opening")}
              value={bookAmount(s.opening_balance, locale)}
            />
            <Fact
              label={t("accounting.banking.closing")}
              value={bookAmount(s.closing_balance, locale)}
            />
            <Fact
              label={t("accounting.banking.matchedLines")}
              value={t("accounting.banking.ofLines", {
                done: s.reconciled_lines,
                total: s.line_count,
              })}
            />
            <Fact
              label={t("accounting.banking.unmatched")}
              value={bookAmount(s.unreconciled_amount, locale)}
            />
          </SubTile>
        </Card>

        <Tile
          id="bs-lines"
          title={t("accounting.banking.lines")}
          span="col-span-12"
          flush
        >
          {lines.isPending ? (
            <div aria-busy="true" className="flex flex-col gap-2 px-6 py-6">
              <Skeleton className="h-4 w-2/3" />
              <Skeleton className="h-4 w-1/2" />
            </div>
          ) : lines.isError ? (
            <div className="px-6 py-6 max-md:px-4">
              <LoadError
                error={lines.error}
                onRetry={() => void lines.refetch()}
              />
            </div>
          ) : rows.length === 0 ? (
            <div className="px-6 py-6 max-md:px-4">
              <Notice title={t("accounting.banking.noLines")} />
            </div>
          ) : (
            <>
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("accounting.banking.line")}</TableHead>
                    <TableHead>{t("accounting.banking.match")}</TableHead>
                    <TableHead>{t("accounting.banking.state")}</TableHead>
                    <TableHead className="text-right">
                      {t("accounting.banking.amount")}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {rows.map((l) => (
                    <TableRow key={l.id}>
                      <TableCell>
                        <span className="block font-medium">
                          {l.description ||
                            t("accounting.banking.row", { n: l.row_number })}
                        </span>
                        <span className="block text-xs text-muted-foreground">
                          {formatDay(l.booking_date, locale)}
                        </span>
                      </TableCell>
                      <TableCell data-m="sub">
                        <span className="flex flex-col gap-1">
                          {l.allocations.map((a) => (
                            <span key={a.journal_line_id}>
                              <Link
                                className={link}
                                href={`/accounting/journals/${a.journal_entry_id}`}
                              >
                                {t("accounting.banking.journal", {
                                  number: a.entry_number ?? "",
                                  amount: bookAmount(a.amount, locale),
                                })}
                              </Link>
                              {a.journal_reversed ? (
                                <span className="ml-2 text-xs text-destructive">
                                  {t("accounting.banking.journalReversed")}
                                </span>
                              ) : null}
                            </span>
                          ))}
                          {mayMatch &&
                          l.allowed_actions.includes("reconcile") ? (
                            <ReconcileDialog
                              statement={s}
                              line={l}
                              outcome={outcome}
                            />
                          ) : null}
                          {mayMatch &&
                          l.allowed_actions.includes("reverse") &&
                          l.reconciliation_id ? (
                            <UnmatchDialog line={l} outcome={outcome} />
                          ) : null}
                        </span>
                      </TableCell>
                      <TableCell data-m="badge">
                        <StatusBadge status={l.state} {...badge(l.state)} />
                      </TableCell>
                      <TableCell
                        data-m="right"
                        className="text-right font-semibold tabular-nums"
                      >
                        {bookAmount(l.amount, locale)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
              {lines.data && lines.data.meta.last_page > 1 ? (
                <div className="border-t border-(--hair) px-5 py-3.5 max-md:px-4 max-md:py-3">
                  <ListPager meta={lines.data.meta as Meta} onPage={setPage} />
                </div>
              ) : null}
            </>
          )}
        </Tile>
      </Bento>
    </>
  )
}

/**
 * Match one statement line: the server's candidate journal lines (capacity left), an amount
 * per picked line (starting at its capacity), then the API checks they add up exactly.
 */
function ReconcileDialog({
  statement,
  line,
  outcome,
}: {
  statement: Statement
  line: StatementLine
  outcome: Outcome
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useBankReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const [picked, setPicked] = React.useState<Record<string, string>>({})
  const candidates = useBankStatementsLineCandidates(
    workspaceId,
    statement.id,
    line.id,
    { page_size: "100" },
    { query: { enabled: Boolean(workspaceId) && a.open } }
  )
  const create = useBankReconciliationsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("accounting.banking.done.matched"))
      },
    },
  })
  const list = (candidates.data?.data ?? []) as Candidate[]
  const allocations = Object.entries(picked).map(
    ([journal_line_id, amount]) => ({
      journal_line_id,
      amount: amount.trim(),
    })
  )
  const errors = fieldErrors(create.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        setPicked({})
        create.reset()
      }}
      button={<Button size="sm" className="self-start max-md:min-h-11" />}
      trigger={t("accounting.banking.reconcile.action")}
      title={t("accounting.banking.reconcile.title")}
      description={t("accounting.banking.reconcile.text", {
        amount: bookAmount(line.amount, locale),
        date: formatDay(line.booking_date, locale),
      })}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: { statement_line_id: line.id, allocations },
            },
          })
        }}
      >
        {candidates.isPending ? (
          <div aria-busy="true" className="flex flex-col gap-2">
            <span className="sr-only">{t("accounting.banking.loading")}</span>
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/2" />
          </div>
        ) : candidates.isError ? (
          <LoadError
            error={candidates.error}
            onRetry={() => void candidates.refetch()}
          />
        ) : list.length === 0 ? (
          <Notice title={t("accounting.banking.reconcile.none")} />
        ) : (
          <fieldset className="flex max-h-[50vh] flex-col gap-2 overflow-y-auto">
            <legend className="mb-1 text-sm font-medium">
              {t("accounting.banking.reconcile.pick")}
            </legend>
            {list.map((c, i) => {
              const on = c.journal_line_id in picked
              const label = t("accounting.banking.candidate", {
                number: c.entry_number ?? "",
                date: formatDay(c.accounting_date, locale),
                left: bookAmount(c.remaining, locale),
              })
              const at = allocations.findIndex(
                (x) => x.journal_line_id === c.journal_line_id
              )
              return (
                <div
                  key={c.journal_line_id}
                  className="flex flex-col gap-1 rounded-md border border-(--hair) p-2"
                >
                  <label className="flex min-h-11 items-start gap-2 text-sm md:min-h-0">
                    <input
                      type="checkbox"
                      className="mt-1"
                      checked={on}
                      onChange={() =>
                        setPicked((all) => {
                          const next = { ...all }
                          if (on) delete next[c.journal_line_id]
                          else next[c.journal_line_id] = c.remaining
                          return next
                        })
                      }
                    />
                    <span className="flex flex-col">
                      <span className="font-medium">{label}</span>
                      <span className="text-xs text-muted-foreground">
                        {c.reason}
                      </span>
                    </span>
                  </label>
                  {on ? (
                    <Field
                      id={`bs-amount-${line.id}-${i}`}
                      label={t("accounting.banking.reconcile.amount")}
                      errors={errors[`payload.allocations.${at}.amount`]}
                    >
                      <Input
                        inputMode="decimal"
                        required
                        value={picked[c.journal_line_id]}
                        onChange={(e) =>
                          setPicked((all) => ({
                            ...all,
                            [c.journal_line_id]: e.target.value,
                          }))
                        }
                      />
                    </Field>
                  ) : null}
                </div>
              )
            })}
          </fieldset>
        )}
        <ApiError
          error={create.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button
            type="submit"
            disabled={allocations.length === 0 || create.isPending}
          >
            {t("accounting.banking.reconcile.action")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

/** Reverse a match: the reason is kept; the line and the journal lines' capacity come back. */
function UnmatchDialog({
  line,
  outcome,
}: {
  line: StatementLine
  outcome: Outcome
}) {
  const t = useTranslations()
  const reasons = useBankReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const reverse = useBankReconciliationsReverse({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("accounting.banking.done.unmatched"))
      },
    },
  })
  const errors = fieldErrors(reverse.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        reverse.reset()
      }}
      button={
        <Button
          variant="outline"
          size="sm"
          className="self-start max-md:min-h-11"
        />
      }
      trigger={t("accounting.banking.unmatch.action")}
      title={t("accounting.banking.unmatch.title")}
      description={t("accounting.banking.unmatch.text")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          reverse.mutate({
            workspace: workspaceId,
            reconciliation: line.reconciliation_id ?? "",
            data: {
              client_ref: a.clientRef,
              expected_version: line.reconciliation_version ?? 0,
              payload: { reason: String(f.get("reason") ?? "").trim() },
            },
          })
        }}
      >
        <Field
          id={`bs-unmatch-${line.id}`}
          label={t("accounting.banking.unmatch.reason")}
          errors={errors["payload.reason"]}
        >
          <Textarea name="reason" required maxLength={1000} />
        </Field>
        <ApiError
          error={reverse.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button
            type="submit"
            variant="destructive"
            disabled={reverse.isPending}
          >
            {t("accounting.banking.unmatch.action")}
          </Button>
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
