"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useAccountingExceptionsList } from "@genesy/api-client/endpoints/accounting"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { buttonVariants } from "@genesy/ui/components/button"
import { formatDateTime } from "@genesy/ui/lib/format"
import {
  CardState,
  ListCard,
  ListSkeleton,
  PageHead,
  PillLinks,
} from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

import { AccountingTabs } from "../shared"
import { useExceptionBadge, useExceptionWord, useMinor, useWhy } from "./shared"

const LIMIT = 50

/**
 * Accounting exceptions (Template List): provider money the book could not post or that left a
 * discrepancy, newest first. Pages by "older" links (the API pages by id, without a total).
 */
export default function ExceptionsPage({
  searchParams,
}: {
  searchParams: Promise<{ state?: string; before?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const badge = useExceptionBadge()
  const word = useExceptionWord()
  const why = useWhy()
  const minor = useMinor()
  const { state, before } = React.use(searchParams)
  const all = state === "all"
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const list = useAccountingExceptionsList(
    workspaceId,
    {
      state: all ? "all" : "pending",
      before: before || undefined,
      limit: LIMIT,
    },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const columns = [
    { label: t("accounting.exceptions.what") },
    { label: t("accounting.exceptions.why.label") },
    { label: t("accounting.exceptions.state") },
    { label: t("accounting.exceptions.occurred") },
    { label: t("accounting.exceptions.amount"), right: true },
  ]
  const rows = list.data?.data ?? []
  const view = all ? "all" : "pending"
  const last = rows.length === LIMIT ? (rows.at(-1)?.id ?? null) : null

  return (
    <>
      <PageHead
        eyebrow={t("shell.accounting")}
        title={t("accounting.exceptions.title")}
        description={t("accounting.exceptions.intro")}
        tabs={<AccountingTabs />}
      />
      <ListCard
        filters={
          <PillLinks
            label={t("accounting.exceptions.filter")}
            links={[
              {
                href: "/accounting/exceptions",
                label: t("accounting.exceptions.pending"),
                active: view === "pending",
              },
              {
                href: "/accounting/exceptions?state=all",
                label: t("accounting.exceptions.all"),
                active: view === "all",
              },
            ]}
          />
        }
        count={null}
        pager={
          before || last ? (
            <nav
              aria-label={t("accounting.exceptions.pages")}
              className="flex justify-between gap-3 text-sm"
            >
              {before ? (
                <Link
                  className={`${buttonVariants({ variant: "outline", size: "sm" })} max-md:min-h-11`}
                  href={`/accounting/exceptions${all ? "?state=all" : ""}`}
                >
                  {t("accounting.exceptions.newest")}
                </Link>
              ) : (
                <span />
              )}
              {last ? (
                <Link
                  className={`${buttonVariants({ variant: "outline", size: "sm" })} max-md:min-h-11`}
                  href={`/accounting/exceptions?${all ? "state=all&" : ""}before=${last}`}
                >
                  {t("accounting.exceptions.older")}
                </Link>
              ) : null}
            </nav>
          ) : null
        }
      >
        {list.isPending ? (
          <ListSkeleton columns={columns} />
        ) : list.isError ? (
          <CardState>
            <LoadError error={list.error} onRetry={() => void list.refetch()} />
          </CardState>
        ) : rows.length === 0 ? (
          <CardState>
            <Empty
              title={
                all
                  ? t("accounting.exceptions.emptyAll")
                  : t("accounting.exceptions.empty")
              }
            />
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
              {rows.map((e) => (
                <TableRow key={e.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline max-md:inline-flex max-md:min-h-11 max-md:items-center"
                      href={`/accounting/exceptions/${e.id}`}
                    >
                      {t("accounting.exceptions.headline", {
                        kind: word("economic", e.economic_kind),
                        provider: e.provider,
                      })}
                    </Link>
                  </TableCell>
                  <TableCell data-m="sub" className="max-w-[36ch] truncate">
                    {why(e)}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge status={e.state} {...badge(e.state)} />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {formatDateTime(e.occurred_at, locale)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {minor(e.observed_amount, e)}
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
