"use client"

import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useAccountingGeneralLedger } from "@genesy/api-client/endpoints/accounting"
import type { CurrencyRef } from "@genesy/api-client/model"
import { ApiError, fieldErrors } from "@genesy/ui/components/api-error"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDay } from "@genesy/ui/lib/format"
import { CardState, ListPager, ListSkeleton } from "@/components/list"
import { Empty } from "@/components/states"

import { showMoney, useReasons } from "./shared"

/** A report that failed: the coded reasons as sentences, date problems by their field. */
export function ReportError({ error }: { error: unknown }) {
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

export function useMoney(currency: CurrencyRef | undefined) {
  const locale = useLocale()
  return (amount: string) =>
    currency ? showMoney(amount, currency, locale) : amount
}

/** One account's general ledger: balances, one page of posted lines, a pager (reports, account page). */
export function Ledger({
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
