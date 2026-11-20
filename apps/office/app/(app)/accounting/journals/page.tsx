"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  useAccountingJournalsList,
  useAccountingSetupGet,
} from "@verzo/api-client/endpoints/accounting"
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

import { AccountingTabs, showMoney, useWord } from "../shared"
import { journalTitle } from "./title"

/** Journals (Template List, as /purchases): newest date first, server totals, pager. */
export default function JournalsPage({
  searchParams,
}: {
  searchParams: Promise<{ page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const word = useWord()
  const mayCreate = useCan("accounting.journal_create")
  const { page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const journals = useAccountingJournalsList(
    workspaceId,
    { page: page || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const setup = useAccountingSetupGet(workspaceId, {
    query: { enabled: Boolean(workspaceId) },
  })
  const columns = [
    { label: t("accounting.journals.journal") },
    { label: t("accounting.journals.reason") },
    { label: t("accounting.journals.state") },
    { label: t("accounting.journals.date") },
    { label: t("accounting.journals.total"), right: true },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.journals.title")}
        tabs={<AccountingTabs />}
        actions={
          mayCreate ? (
            <Link href="/accounting/journals/new" className={buttonVariants()}>
              {t("accounting.journals.new")}
            </Link>
          ) : null
        }
      />
      {setup.data && setup.data.data.state !== "active" ? (
        <Notice title={t("accounting.journals.notActive")} />
      ) : null}
      <ListCard
        count={
          journals.data
            ? t("accounting.journals.count", {
                total: journals.data.meta.total,
              })
            : null
        }
        pager={
          journals.data?.data.length ? (
            <ListPager
              meta={journals.data.meta}
              onPage={(p) => router.push(`?page=${p}`)}
            />
          ) : null
        }
      >
        {journals.isPending ? (
          <ListSkeleton columns={columns} />
        ) : journals.isError ? (
          <CardState>
            <LoadError
              error={journals.error}
              onRetry={() => void journals.refetch()}
            />
          </CardState>
        ) : journals.data.data.length === 0 ? (
          <CardState>
            <Empty title={t("accounting.journals.empty")} />
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
              {journals.data.data.map((j) => (
                <TableRow key={j.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline"
                      href={`/accounting/journals/${j.id}`}
                    >
                      {journalTitle(t, j)}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub" className="max-w-[28ch] truncate">
                    {j.reason}
                  </TableCell>
                  <TableCell data-m="badge">
                    <span className="flex flex-wrap gap-1.5">
                      <StatusBadge
                        status={j.state}
                        label={word("states", j.state)}
                        tone={j.state === "posted" ? "teal" : "gold"}
                      />
                      {j.kind !== "manual" ? (
                        <StatusBadge
                          status={j.kind}
                          label={word("kinds", j.kind)}
                          tone="slate"
                        />
                      ) : null}
                    </span>
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {formatDay(j.accounting_date, locale)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {showMoney(j.totals.debit, j.currency, locale)}
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
