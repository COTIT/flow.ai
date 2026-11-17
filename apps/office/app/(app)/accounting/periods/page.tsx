"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getAccountingPeriodsListQueryKey,
  getAccountingYearsListQueryKey,
  useAccountingPeriodsCreate,
  useAccountingPeriodsList,
  useAccountingYearsCreate,
  useAccountingYearsList,
} from "@genesy/api-client/endpoints/accounting"
import type { YearResource } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { DialogClose, DialogFooter } from "@genesy/ui/components/dialog"
import { Input } from "@genesy/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDay } from "@genesy/ui/lib/format"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

import {
  AccountingTabs,
  Shell,
  useAction,
  useReasons,
  useWord,
  type Outcome,
} from "../shared"

/**
 * Fiscal years and their periods (Template List): one card per year, its periods as rows.
 * Ranges are [start, end): the end date is the first day after.
 */
export default function PeriodsPage() {
  const t = useTranslations()
  const locale = useLocale()
  const word = useWord()
  const queryClient = useQueryClient()
  const mayConfigure = useCan("accounting.configure")
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  // shortcut: first 100 years and periods; page these when a book holds more.
  const years = useAccountingYearsList(
    workspaceId,
    { page_size: "100" },
    enabled
  )
  const periods = useAccountingPeriodsList(
    workspaceId,
    { page_size: "100" },
    enabled
  )
  const done = useDone()
  const refresh = () => {
    for (const queryKey of [
      getAccountingYearsListQueryKey(workspaceId),
      getAccountingPeriodsListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const outcome: Outcome = {
    done: (message) => {
      done.show(message)
      refresh()
    },
    stale: refresh,
  }
  const day = (d: string) => formatDay(d, locale)
  const columns = [
    { label: t("accounting.periods.start") },
    { label: t("accounting.periods.end") },
    { label: t("accounting.periods.state") },
  ]
  const list = years.data?.data ?? []

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.periods.title")}
        description={t("accounting.periods.rangeHint")}
        tabs={<AccountingTabs />}
        actions={
          mayConfigure ? (
            <>
              {list.length ? (
                <PeriodDialog
                  years={list}
                  outcome={outcome}
                  button={<Button variant="outline" />}
                />
              ) : null}
              <YearDialog outcome={outcome} button={<Button />} />
            </>
          ) : null
        }
      />
      <Done message={done.message} />
      {years.isPending || periods.isPending ? (
        <ListCard>
          <ListSkeleton columns={columns} rows={3} />
        </ListCard>
      ) : years.isError || periods.isError ? (
        <ListCard>
          <CardState>
            <LoadError
              error={years.error ?? periods.error}
              onRetry={() => {
                void years.refetch()
                void periods.refetch()
              }}
            />
          </CardState>
        </ListCard>
      ) : list.length === 0 ? (
        <ListCard>
          <CardState>
            <Empty title={t("accounting.periods.emptyYears")} />
          </CardState>
        </ListCard>
      ) : (
        list.map((y) => {
          const own = periods.data.data.filter((p) => p.fiscal_year_id === y.id)
          return (
            <ListCard
              key={y.id}
              filters={
                <h2 className="font-semibold">
                  {t("accounting.periods.year", {
                    start: day(y.start_date),
                    end: day(y.end_date),
                  })}
                </h2>
              }
              count={
                <StatusBadge
                  status={y.state}
                  label={word("periodStates", y.state)}
                />
              }
            >
              {own.length === 0 ? (
                <CardState>
                  <Empty title={t("accounting.periods.emptyPeriods")} />
                </CardState>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      {columns.map((c) => (
                        <TableHead key={c.label}>{c.label}</TableHead>
                      ))}
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {own.map((p) => (
                      <TableRow key={p.id}>
                        <TableCell className="font-semibold">
                          {day(p.start_date)}
                        </TableCell>
                        <TableCell data-m="sub">{day(p.end_date)}</TableCell>
                        <TableCell data-m="badge">
                          <StatusBadge
                            status={p.state}
                            label={word("periodStates", p.state)}
                          />
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </ListCard>
          )
        })
      )}
    </>
  )
}

function YearDialog({
  outcome,
  button,
}: {
  outcome: Outcome
  button: React.ReactElement
}) {
  const t = useTranslations()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const create = useAccountingYearsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.accounting.yearCreated"))
      },
    },
  })
  const errors = fieldErrors(create.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        create.reset()
      }}
      button={button}
      trigger={t("accounting.periods.newYear")}
      title={t("accounting.periods.newYear")}
      description={t("accounting.periods.newYearText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: {
                start_date: String(f.get("start_date") ?? ""),
                end_date: String(f.get("end_date") ?? ""),
              },
            },
          })
        }}
      >
        <Field
          id="yr-start"
          label={t("accounting.periods.start")}
          errors={errors["payload.start_date"]}
        >
          <Input type="date" name="start_date" required />
        </Field>
        <Field
          id="yr-end"
          label={t("accounting.periods.end")}
          errors={errors["payload.end_date"]}
        >
          <Input type="date" name="end_date" required />
        </Field>
        <ApiError
          error={create.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={create.isPending}>
            {t("accounting.periods.createYear")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}

function PeriodDialog({
  years,
  outcome,
  button,
}: {
  years: YearResource[]
  outcome: Outcome
  button: React.ReactElement
}) {
  const t = useTranslations()
  const locale = useLocale()
  const reasons = useReasons()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const a = useAction(outcome)
  const create = useAccountingPeriodsCreate({
    ...a.options,
    mutation: {
      ...a.options.mutation,
      onSuccess: () => {
        a.setOpen(false)
        outcome.done(t("done.accounting.periodCreated"))
      },
    },
  })
  const errors = fieldErrors(create.error)
  return (
    <Shell
      open={a.open}
      setOpen={a.setOpen}
      onOpen={() => {
        a.key.renew()
        create.reset()
      }}
      button={button}
      trigger={t("accounting.periods.newPeriod")}
      title={t("accounting.periods.newPeriod")}
      description={t("accounting.periods.newPeriodText")}
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault()
          const f = new FormData(e.currentTarget)
          create.mutate({
            workspace: workspaceId,
            data: {
              client_ref: a.clientRef,
              payload: {
                fiscal_year_id: String(f.get("fiscal_year_id") ?? ""),
                start_date: String(f.get("start_date") ?? ""),
                end_date: String(f.get("end_date") ?? ""),
              },
            },
          })
        }}
      >
        <Field
          id="pd-year"
          label={t("accounting.periods.fiscalYear")}
          errors={errors["payload.fiscal_year_id"]}
        >
          <select
            name="fiscal_year_id"
            className={selectClass}
            defaultValue={years[0]?.id}
          >
            {years.map((y) => (
              <option key={y.id} value={y.id}>
                {t("accounting.periods.year", {
                  start: formatDay(y.start_date, locale),
                  end: formatDay(y.end_date, locale),
                })}
              </option>
            ))}
          </select>
        </Field>
        <Field
          id="pd-start"
          label={t("accounting.periods.start")}
          errors={errors["payload.start_date"]}
        >
          <Input type="date" name="start_date" required />
        </Field>
        <Field
          id="pd-end"
          label={t("accounting.periods.end")}
          errors={errors["payload.end_date"]}
        >
          <Input type="date" name="end_date" required />
        </Field>
        <ApiError
          error={create.error}
          fallback={t("common.error")}
          codes={reasons}
        />
        <DialogFooter>
          <DialogClose render={<Button type="button" variant="outline" />}>
            {t("actions.cancel")}
          </DialogClose>
          <Button type="submit" disabled={create.isPending}>
            {t("accounting.periods.createPeriod")}
          </Button>
        </DialogFooter>
      </form>
    </Shell>
  )
}
