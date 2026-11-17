"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useAccountingAccountsList,
  useAccountingBalanceSheet,
  useAccountingGeneralLedger,
  useAccountingProfitAndLoss,
  useAccountingTrialBalance,
} from "@genesy/api-client/endpoints/accounting"
import type { AmountRow, CurrencyRef } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
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
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { Empty } from "@/components/states"

import { AccountingTabs, showMoney, thisMonth, useReasons } from "../shared"

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
  const month = thisMonth()
  const from = q.from ?? month.from
  const to = q.to ?? month.to
  const asOf = q.as_of ?? to
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const [kind, setKind] = React.useState<Report>(report)
  // shortcut: the first 100 accounts in the ledger's picker; page these when a book holds more.
  const accounts = useAccountingAccountsList(
    workspaceId,
    { page_size: "100" },
    { query: { enabled: Boolean(workspaceId) && kind === "general-ledger" } }
  )
  const go = (next: Query) => {
    const params = new URLSearchParams()
    for (const [k, v] of Object.entries({ ...q, ...next }))
      if (v) params.set(k, v)
    router.push(`?${params}`)
  }

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
                <select
                  key={accounts.data ? "loaded" : "loading"}
                  name="account"
                  className={selectClass}
                  required
                  defaultValue={q.account ?? ""}
                >
                  <option value="" disabled>
                    {t("accounting.reports.chooseAccount")}
                  </option>
                  {accounts.data?.data.map((a) => (
                    <option key={a.id} value={a.id}>
                      {a.code} {a.name}
                    </option>
                  ))}
                </select>
              </Field>
            ) : null}
            <Button type="submit">{t("accounting.reports.show")}</Button>
          </form>
        }
      >
        {report === "trial-balance" ? (
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

/** A report that failed: the coded reasons as sentences, date problems by their field. */
function ReportError({ error }: { error: unknown }) {
  const t = useTranslations()
  const reasons = useReasons()
  const fields = Object.values(fieldErrors(error)).flat()
  return (
    <CardState>
      {fields.length ? (
        <ApiError
          error={{ response: { data: { message: fields.join(" ") } } }}
          fallback={t("common.error")}
        />
      ) : (
        <ApiError error={error} fallback={t("common.error")} codes={reasons} />
      )}
    </CardState>
  )
}

function useMoney(currency: CurrencyRef | undefined) {
  const locale = useLocale()
  return (amount: string) =>
    currency ? showMoney(amount, currency, locale) : amount
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

function Ledger({
  account,
  from,
  to,
  page,
  onPage,
}: {
  account: string
  from: string
  to: string
  page?: string
  onPage: (page: number) => void
}) {
  const t = useTranslations()
  const locale = useLocale()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const gl = useAccountingGeneralLedger(
    workspaceId,
    { account_id: account, from, to, page },
    { query: { enabled: Boolean(workspaceId), retry: false } }
  )
  const money = useMoney(gl.data?.data.functional_currency)
  const cols = [
    { label: t("accounting.reports.date") },
    { label: t("accounting.reports.journal") },
    { label: t("accounting.reports.debit"), right: true },
    { label: t("accounting.reports.credit"), right: true },
    { label: t("accounting.reports.balance"), right: true },
  ]
  if (gl.isPending) return <ListSkeleton columns={cols} />
  if (gl.isError) return <ReportError error={gl.error} />
  const d = gl.data.data
  const facts = [
    [t("accounting.reports.opening"), d.opening_balance],
    [t("accounting.reports.debit"), d.debit],
    [t("accounting.reports.credit"), d.credit],
    [t("accounting.reports.closing"), d.closing_balance],
  ] as const
  return (
    <>
      <div className="flex flex-col gap-2 px-6 pb-4 max-md:px-4">
        <p className="font-semibold">
          {d.account.code} {d.account.name}
        </p>
        <dl className="grid grid-cols-4 gap-3 text-[13px] max-md:grid-cols-2">
          {facts.map(([label, value]) => (
            <div key={label}>
              <dt className="text-muted-foreground">{label}</dt>
              <dd className="font-semibold tabular-nums">{money(value)}</dd>
            </div>
          ))}
        </dl>
        <p className="text-xs text-muted-foreground">
          {t("accounting.reports.balanceHint")}
        </p>
      </div>
      {d.lines.data.length === 0 ? (
        <CardState>
          <Empty title={t("accounting.reports.empty")} />
        </CardState>
      ) : (
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
            {d.lines.data.map((l) => (
              <TableRow key={`${l.entry_id}-${l.line_no}`}>
                <TableCell data-m="hide" className="text-muted-foreground">
                  {formatDay(l.accounting_date, locale)}
                </TableCell>
                <TableCell>
                  <Link
                    className="font-semibold underline-offset-4 hover:underline"
                    href={`/accounting/journals/${l.entry_id}`}
                  >
                    {t("accounting.journals.numbered", { number: l.number })}
                  </Link>
                  <span className="block max-w-[32ch] truncate text-xs text-muted-foreground">
                    {l.reason}
                  </span>
                </TableCell>
                <TableCell data-m="hide" className="text-right tabular-nums">
                  {/^0(\.0+)?$/.test(l.debit) ? "" : money(l.debit)}
                </TableCell>
                <TableCell data-m="hide" className="text-right tabular-nums">
                  {/^0(\.0+)?$/.test(l.credit) ? "" : money(l.credit)}
                </TableCell>
                <TableCell
                  data-m="right"
                  className="text-right font-semibold tabular-nums"
                >
                  {money(l.balance)}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      {d.lines.data.length ? (
        <div className="border-t border-(--hair) px-5 py-3.5 max-md:px-4 max-md:py-3">
          <ListPager meta={d.lines.meta} onPage={onPage} />
        </div>
      ) : null}
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
