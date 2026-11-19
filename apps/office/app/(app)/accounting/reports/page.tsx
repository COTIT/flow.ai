"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useAccountingBalanceSheet,
  useAccountingProfitAndLoss,
  useAccountingTrialBalance,
} from "@genesy/api-client/endpoints/accounting"
import type { AmountRow } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import {
  Table,
  TableBody,
  TableCell,
  TableFooter,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDay } from "@genesy/ui/lib/format"
import { Field, selectClass } from "@/components/field"
import { CardState, ListCard, ListSkeleton, PageHead } from "@/components/list"
import { Empty } from "@/components/states"

import { Ledger, ReportError, useMoney } from "../ledger"
import { AccountPicker } from "@/components/pickers"
import { LoadError } from "@/components/states"

import { AccountingTabs, useBookMonth } from "../shared"

const reports = [
  "trial-balance",
  "general-ledger",
  "profit-and-loss",
  "balance-sheet",
] as const
type Report = (typeof reports)[number]
type Query = {
  report?: string
  from?: string
  to?: string
  as_of?: string
  account?: string
  page?: string
}

/**
 * The four book reports (Template List): pick one, its dates (and account), apply. Every
 * figure is the API's string in the book's currency: formatted, never added up here.
 */
export default function ReportsPage({
  searchParams,
}: {
  searchParams: Promise<Query>
}) {
  const t = useTranslations()
  const router = useRouter()
  const q = React.use(searchParams)
  const report: Report = reports.includes(q.report as Report)
    ? (q.report as Report)
    : "trial-balance"
  const book = useBookMonth()
  const from = q.from ?? book.month?.from ?? ""
  const to = q.to ?? book.month?.to ?? ""
  const asOf = q.as_of ?? to
  const [kind, setKind] = React.useState<Report>(report)
  const [account, setAccount] = React.useState(q.account ?? "")
  const go = (next: Query) => {
    const params = new URLSearchParams()
    for (const [k, v] of Object.entries({ ...q, ...next }))
      if (v) params.set(k, v)
    router.push(`?${params}`)
  }

  if (book.setup.isError)
    return (
      <LoadError
        error={book.setup.error}
        onRetry={() => void book.setup.refetch()}
      />
    )

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.reports.title")}
        tabs={<AccountingTabs />}
      />
      <ListCard
        filters={
          <form
            key={`${from}|${to}`}
            className="flex flex-wrap items-end gap-3 max-md:w-full max-md:flex-col max-md:items-stretch"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              const text = (k: string) => String(f.get(k) ?? "") || undefined
              go({
                report: text("report"),
                from: text("from"),
                to: text("to"),
                as_of: text("as_of"),
                account: text("account"),
                page: undefined,
              })
            }}
          >
            <Field id="rp-report" label={t("accounting.reports.report")}>
              <select
                name="report"
                className={selectClass}
                value={kind}
                onChange={(e) => setKind(e.target.value as Report)}
              >
                {reports.map((r) => (
                  <option key={r} value={r}>
                    {t(`accounting.reports.kinds.${r}`)}
                  </option>
                ))}
              </select>
            </Field>
            {kind === "balance-sheet" ? (
              <Field id="rp-asof" label={t("accounting.reports.asOf")}>
                <Input type="date" name="as_of" required defaultValue={asOf} />
              </Field>
            ) : (
              <>
                <Field id="rp-from" label={t("accounting.reports.from")}>
                  <Input type="date" name="from" required defaultValue={from} />
                </Field>
                <Field id="rp-to" label={t("accounting.reports.to")}>
                  <Input type="date" name="to" required defaultValue={to} />
                </Field>
              </>
            )}
            {kind === "general-ledger" ? (
              <Field id="rp-account" label={t("accounting.reports.account")}>
                <AccountPicker
                  name="account"
                  value={account}
                  onChange={setAccount}
                  placeholder={t("accounting.reports.chooseAccount")}
                />
              </Field>
            ) : null}
            <Button type="submit">{t("accounting.reports.show")}</Button>
          </form>
        }
      >
        {!from || !to ? (
          <ListSkeleton columns={[{ label: "" }, { label: "", right: true }]} />
        ) : report === "trial-balance" ? (
          <TrialBalance from={from} to={to} />
        ) : report === "general-ledger" ? (
          q.account ? (
            <Ledger
              account={q.account}
              from={from}
              to={to}
              page={q.page}
              onPage={(p) => go({ page: String(p) })}
            />
          ) : (
            <CardState>
              <Empty title={t("accounting.reports.pickAccount")} />
            </CardState>
          )
        ) : report === "profit-and-loss" ? (
          <ProfitAndLoss from={from} to={to} />
        ) : (
          <BalanceSheet asOf={asOf} />
        )}
      </ListCard>
    </>
  )
}

function Range({ from, to }: { from: string; to: string }) {
  const t = useTranslations()
  const locale = useLocale()
  return (
    <p className="px-6 pb-3 text-[13px] text-muted-foreground max-md:px-4">
      {t("accounting.reports.range", {
        from: formatDay(from, locale),
        to: formatDay(to, locale),
      })}
    </p>
  )
}

function TrialBalance({ from, to }: { from: string; to: string }) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const tb = useAccountingTrialBalance(
    workspaceId,
    { from, to },
    { query: { enabled: Boolean(workspaceId), retry: false } }
  )
  const money = useMoney(tb.data?.data.functional_currency)
  const cols = [
    { label: t("accounting.reports.account") },
    { label: t("accounting.reports.openingDebit"), right: true },
    { label: t("accounting.reports.openingCredit"), right: true },
    { label: t("accounting.reports.debit"), right: true },
    { label: t("accounting.reports.credit"), right: true },
    { label: t("accounting.reports.closingDebit"), right: true },
    { label: t("accounting.reports.closingCredit"), right: true },
  ]
  if (tb.isPending) return <ListSkeleton columns={cols} />
  if (tb.isError) return <ReportError error={tb.error} />
  const d = tb.data.data
  if (d.rows.length === 0)
    return (
      <CardState>
        <Empty title={t("accounting.reports.empty")} />
      </CardState>
    )
  const keys = [
    "opening_debit",
    "opening_credit",
    "debit",
    "credit",
    "closing_debit",
    "closing_credit",
  ] as const
  return (
    <>
      <Range from={d.from} to={d.to} />
      <Table>
        <TableHeader>
          <TableRow>
            {cols.map((c) => (
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
          {d.rows.map((r) => (
            <TableRow key={r.account.id}>
              <TableCell className="font-semibold">
                {r.account.code} {r.account.name}
              </TableCell>
              {keys.map((k) => (
                <TableCell
                  key={k}
                  data-m={
                    k === "closing_debit"
                      ? "sub"
                      : k === "closing_credit"
                        ? "right"
                        : "hide"
                  }
                  className="text-right tabular-nums"
                >
                  {money(r[k])}
                </TableCell>
              ))}
            </TableRow>
          ))}
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell className="font-semibold">
              {t("accounting.reports.totals")}
            </TableCell>
            {keys.map((k) => (
              <TableCell
                key={k}
                data-m={
                  k === "closing_debit"
                    ? "sub"
                    : k === "closing_credit"
                      ? "right"
                      : "hide"
                }
                className="text-right font-semibold tabular-nums"
              >
                {money(d.totals[k])}
              </TableCell>
            ))}
          </TableRow>
        </TableFooter>
      </Table>
    </>
  )
}

function Section({
  title,
  rows,
  total,
  money,
}: {
  title: string
  rows: AmountRow[]
  total: string
  money: (a: string) => string
}) {
  return (
    <>
      <TableRow>
        <TableHead
          colSpan={2}
          className="pt-4 text-xs font-bold tracking-[0.08em] uppercase"
        >
          {title}
        </TableHead>
      </TableRow>
      {rows.map((r) => (
        <TableRow key={r.account.id}>
          <TableCell>
            {r.account.code} {r.account.name}
          </TableCell>
          <TableCell data-m="right" className="text-right tabular-nums">
            {money(r.amount)}
          </TableCell>
        </TableRow>
      ))}
      <TableRow>
        <TableCell className="font-semibold">{title}</TableCell>
        <TableCell
          data-m="right"
          className="text-right font-semibold tabular-nums"
        >
          {money(total)}
        </TableCell>
      </TableRow>
    </>
  )
}

function ProfitAndLoss({ from, to }: { from: string; to: string }) {
  const t = useTranslations()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const pl = useAccountingProfitAndLoss(
    workspaceId,
    { from, to },
    { query: { enabled: Boolean(workspaceId), retry: false } }
  )
  const money = useMoney(pl.data?.data.functional_currency)
  const cols = [
    { label: t("accounting.reports.account") },
    { label: t("accounting.reports.amount"), right: true },
  ]
  if (pl.isPending) return <ListSkeleton columns={cols} />
  if (pl.isError) return <ReportError error={pl.error} />
  const d = pl.data.data
  if (d.income.length === 0 && d.expenses.length === 0)
    return (
      <CardState>
        <Empty title={t("accounting.reports.empty")} />
      </CardState>
    )
  return (
    <>
      <Range from={d.from} to={d.to} />
      <Table>
        <TableBody>
          <Section
            title={t("accounting.reports.income")}
            rows={d.income}
            total={d.total_income}
            money={money}
          />
          <Section
            title={t("accounting.reports.expenses")}
            rows={d.expenses}
            total={d.total_expenses}
            money={money}
          />
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell className="font-semibold">
              {t("accounting.reports.netProfit")}
            </TableCell>
            <TableCell
              data-m="right"
              className="text-right font-semibold tabular-nums"
            >
              {money(d.net_profit)}
            </TableCell>
          </TableRow>
        </TableFooter>
      </Table>
    </>
  )
}

function BalanceSheet({ asOf }: { asOf: string }) {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const bs = useAccountingBalanceSheet(
    workspaceId,
    { as_of: asOf },
    { query: { enabled: Boolean(workspaceId), retry: false } }
  )
  const money = useMoney(bs.data?.data.functional_currency)
  const cols = [
    { label: t("accounting.reports.account") },
    { label: t("accounting.reports.amount"), right: true },
  ]
  if (bs.isPending) return <ListSkeleton columns={cols} />
  if (bs.isError) return <ReportError error={bs.error} />
  const d = bs.data.data
  if (
    d.assets.length === 0 &&
    d.liabilities.length === 0 &&
    d.equity.length === 0
  )
    return (
      <CardState>
        <Empty title={t("accounting.reports.empty")} />
      </CardState>
    )
  return (
    <>
      <p className="px-6 pb-3 text-[13px] text-muted-foreground max-md:px-4">
        {t("accounting.reports.asOfDay", { date: formatDay(d.as_of, locale) })}
      </p>
      <Table>
        <TableBody>
          <Section
            title={t("accounting.reports.assets")}
            rows={d.assets}
            total={d.total_assets}
            money={money}
          />
          <Section
            title={t("accounting.reports.liabilities")}
            rows={d.liabilities}
            total={d.total_liabilities}
            money={money}
          />
          <Section
            title={t("accounting.reports.equity")}
            rows={d.equity}
            total={d.total_equity}
            money={money}
          />
          <TableRow>
            <TableCell>{t("accounting.reports.currentEarnings")}</TableCell>
            <TableCell data-m="right" className="text-right tabular-nums">
              {money(d.current_earnings)}
            </TableCell>
          </TableRow>
        </TableBody>
        <TableFooter>
          <TableRow>
            <TableCell className="font-semibold">
              {t("accounting.reports.totalLiabilitiesAndEquity")}
            </TableCell>
            <TableCell
              data-m="right"
              className="text-right font-semibold tabular-nums"
            >
              {money(d.total_liabilities_and_equity)}
            </TableCell>
          </TableRow>
        </TableFooter>
      </Table>
    </>
  )
}
