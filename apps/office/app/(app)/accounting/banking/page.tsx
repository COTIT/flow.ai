"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import { useAccountingBankAccountsList } from "@verzo/api-client/endpoints/accounting"
import { useBankStatementsList } from "@verzo/api-client/endpoints/bank-statements"
import { buttonVariants } from "@verzo/ui/components/button"
import { Notice } from "@verzo/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@verzo/ui/components/table"
import { formatDay } from "@verzo/ui/lib/format"
import {
  CardState,
  ListCard,
  ListPager,
  ListSkeleton,
  PageHead,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"
import { useCan } from "@/lib/can"

import { AccountingTabs, bookAmount } from "../shared"
import { useBankBadge, type Statement } from "./shared"

/**
 * Bank statements (Template List, as /accounting/journals): newest first, with what is still
 * unmatched. Statements arrive through imports; matching happens per line on the statement.
 */
export default function BankingPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const badge = useBankBadge()
  const mayImport = useCan("import.list")
  const { page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const statements = useBankStatementsList(
    workspaceId,
    { page: page || undefined },
    enabled
  )
  // One read for every bank account name (the API caps the list at 500).
  const banks = useAccountingBankAccountsList(workspaceId, enabled)
  const bank = (id: string) =>
    banks.data?.data.find((b) => b.id === id)?.name ?? ""
  const columns = [
    { label: t("accounting.banking.statement") },
    { label: t("accounting.banking.bank") },
    { label: t("accounting.banking.state") },
    { label: t("accounting.banking.period") },
    { label: t("accounting.banking.unmatched"), right: true },
  ]
  const rows = (statements.data?.data ?? []) as Statement[]

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.banking.title")}
        tabs={<AccountingTabs />}
        actions={
          mayImport ? (
            <Link
              href="/settings/imports"
              className={`${buttonVariants({ variant: "outline" })} max-md:min-h-11`}
            >
              {t("accounting.banking.import")}
            </Link>
          ) : null
        }
      />
      <Notice title={t("accounting.banking.importNote")} />
      <ListCard
        count={
          statements.data
            ? t("accounting.banking.count", {
                total: statements.data.meta.total,
              })
            : null
        }
        pager={
          rows.length && statements.data ? (
            <ListPager
              meta={statements.data.meta}
              onPage={(p) => router.push(`?page=${p}`)}
            />
          ) : null
        }
      >
        {statements.isPending ? (
          <ListSkeleton columns={columns} />
        ) : statements.isError ? (
          <CardState>
            <LoadError
              error={statements.error}
              onRetry={() => void statements.refetch()}
            />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <Empty title={t("accounting.banking.empty")} />
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
              {rows.map((s) => (
                <TableRow key={s.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
                      href={`/accounting/banking/${s.id}`}
                    >
                      {s.reference}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub">{bank(s.bank_account_id)}</TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge status={s.state} {...badge(s.state)} />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {t("accounting.banking.range", {
                      from: formatDay(s.start_date, locale),
                      to: formatDay(s.end_date, locale),
                    })}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {bookAmount(s.unreconciled_amount, locale)}
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
