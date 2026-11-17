"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getAccountingJournalsGetQueryKey,
  getAccountingJournalsListQueryKey,
  useAccountingAccountsList,
  useAccountingJournalsGet,
} from "@genesy/api-client/endpoints/accounting"
import { Button, buttonVariants } from "@genesy/ui/components/button"
import { Card } from "@genesy/ui/components/card"
import { Notice } from "@genesy/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDateTime, formatDay } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import { showMoney, useWord, type Outcome } from "../../shared"
import { journalTitle } from "../title"
import { PostDialog, ReverseDialog } from "./dialogs"

const zero = (a: string) => /^0(\.0+)?$/.test(a)

/** A journal (Detail tiles, as a purchase order): lines + server totals, next step, links. */
export default function JournalPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = useWord()
  const rights = useRights()
  const queryClient = useQueryClient()
  const { id } = React.use(params)
  const { done: arrived } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const enabled = { query: { enabled: Boolean(workspaceId) } }
  const journal = useAccountingJournalsGet(workspaceId, id, enabled)
  // shortcut: names for the first 100 accounts; others show their code only.
  const accounts = useAccountingAccountsList(
    workspaceId,
    { page_size: "100" },
    enabled
  )
  const done = useDone(
    arrived === "created" || arrived === "saved"
      ? t(`done.accounting.${arrived}`)
      : null
  )
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getAccountingJournalsGetQueryKey(workspaceId, id),
      getAccountingJournalsListQueryKey(workspaceId),
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

  if (journal.isPending) return <DetailSkeleton />
  if (journal.isError)
    return (
      <LoadError
        error={journal.error}
        onRetry={() => void journal.refetch()}
        backHref="/accounting/journals"
      />
    )
  const j = journal.data.data
  const money = (a: string) => showMoney(a, j.currency, locale)
  const book = j.functional_currency
  const fx = j.state === "posted" && book && book.id !== j.currency.id
  const name = (accountId: string) =>
    accounts.data?.data.find((a) => a.id === accountId)?.name ?? ""
  const mayPost =
    j.allowed_actions.includes("post") && rights.may("accounting.post")
  const mayEdit =
    j.allowed_actions.includes("update") &&
    rights.may("accounting.journal_create")
  const mayReverse =
    j.allowed_actions.includes("reverse") && rights.may("accounting.reverse")

  return (
    <>
      <PageHead
        back={{
          href: "/accounting/journals",
          label: t("accounting.journals.title"),
        }}
        title={journalTitle(t, j)}
        description={`${formatDay(j.accounting_date, locale)} · ${j.reason}`}
        actions={
          <span className="flex flex-wrap gap-1.5">
            <StatusBadge
              status={j.state}
              label={word("states", j.state)}
              tone={j.state === "posted" ? "teal" : "gold"}
            />
            <StatusBadge
              status={j.kind}
              label={word("kinds", j.kind)}
              tone="slate"
            />
          </span>
        }
      />
      {stale ? (
        <Notice
          variant="warning"
          title={t("accounting.reasons.stale_document")}
        />
      ) : null}
      <Bento>
        <Tile
          id="jn-lines"
          title={t("accounting.journals.lines")}
          span="col-span-8 row-span-2 max-md:row-span-1"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("accounting.journals.account")}</TableHead>
                <TableHead className="text-right">
                  {t("accounting.journals.debit")}
                </TableHead>
                <TableHead className="text-right">
                  {t("accounting.journals.credit")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {j.lines.map((l) => (
                <TableRow key={l.line_no}>
                  <TableCell>
                    <span className="block font-semibold">
                      {l.account_code} {name(l.account_id)}
                    </span>
                    {l.source_ref ? (
                      <span className="block text-xs text-muted-foreground">
                        {l.source_ref}
                      </span>
                    ) : null}
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {zero(l.debit) ? "" : money(l.debit)}
                    {fx && !zero(l.functional_debit) ? (
                      <span className="block text-xs text-muted-foreground">
                        {showMoney(l.functional_debit, book, locale)}
                      </span>
                    ) : null}
                  </TableCell>
                  <TableCell data-m="right" className="text-right tabular-nums">
                    {zero(l.credit) ? "" : money(l.credit)}
                    {fx && !zero(l.functional_credit) ? (
                      <span className="block text-xs text-muted-foreground">
                        {showMoney(l.functional_credit, book, locale)}
                      </span>
                    ) : null}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {/* The server's totals as strings: formatted, never summed here. */}
          <div className="mt-auto flex flex-col gap-1 border-t border-(--hair) px-5 py-4">
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">
                {t("accounting.journals.totalDebit")}
              </span>
              <span className="tabular-nums">{money(j.totals.debit)}</span>
            </p>
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">
                {t("accounting.journals.totalCredit")}
              </span>
              <span className="tabular-nums">{money(j.totals.credit)}</span>
            </p>
            {fx ? (
              <p className="flex justify-between text-[13px]">
                <span className="text-muted-foreground">
                  {t("accounting.post.inBook", { currency: book.code })}
                </span>
                <span className="tabular-nums">
                  {showMoney(j.totals.functional_debit, book, locale)}
                </span>
              </p>
            ) : null}
            <p className="text-xs text-muted-foreground">
              {t("accounting.journals.serverAmounts")}
            </p>
          </div>
        </Tile>

        <SideCard id="jn-next" title={t("accounting.journals.nextStep")}>
          <Done message={done.message} />
          {mayPost ? (
            <>
              <p className="text-[13px]">
                {t("accounting.journals.next.post")}
              </p>
              <PostDialog
                journal={j}
                outcome={outcome}
                button={<Button size="lg" className="w-full" />}
              />
            </>
          ) : null}
          {mayEdit ? (
            <Link
              href={`/accounting/journals/${j.id}/edit`}
              className={buttonVariants({
                variant: "outline",
                className: "w-full",
              })}
            >
              {t("accounting.journals.edit")}
            </Link>
          ) : null}
          {mayReverse ? (
            <>
              <p className="text-[13px]">
                {t("accounting.journals.next.reverse")}
              </p>
              <ReverseDialog
                journal={j}
                outcome={outcome}
                button={<Button variant="outline" className="w-full" />}
              />
            </>
          ) : null}
          {!mayPost && !mayEdit && !mayReverse ? (
            <Notice
              title={
                j.state === "posted"
                  ? t("accounting.journals.postedInfo")
                  : t("accounting.journals.nothingToDo")
              }
            />
          ) : null}
        </SideCard>

        <SideCard id="jn-facts" title={t("accounting.journals.details")}>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact
              label={t("accounting.journals.date")}
              value={formatDay(j.accounting_date, locale)}
            />
            <Fact
              label={t("accounting.journals.currency")}
              value={j.currency.code}
            />
            {j.evidence_ref ? (
              <Fact
                label={t("accounting.journals.evidence")}
                value={j.evidence_ref}
              />
            ) : null}
            {j.posted_at ? (
              <Fact
                label={t("accounting.journals.postedAt")}
                value={formatDateTime(j.posted_at, locale)}
              />
            ) : null}
          </SubTile>
          <p className="text-[13px] break-words">{j.reason}</p>
          {j.reversal_of ? (
            <Link
              className="text-[13px] font-medium underline-offset-4 hover:underline"
              href={`/accounting/journals/${j.reversal_of}`}
            >
              {t("accounting.journals.reversalOf")}
            </Link>
          ) : null}
          {j.reversed_by ? (
            <Link
              className="text-[13px] font-medium underline-offset-4 hover:underline"
              href={`/accounting/journals/${j.reversed_by}`}
            >
              {t("accounting.journals.reversedBy")}
            </Link>
          ) : null}
        </SideCard>
      </Bento>
    </>
  )
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <p className="flex items-center justify-between gap-3">
      <span className="text-muted-foreground">{label}</span>
      <span className="break-words">{value}</span>
    </p>
  )
}

function SideCard({
  id,
  title,
  children,
}: {
  id: string
  title: string
  children: React.ReactNode
}) {
  return (
    <Card
      role="region"
      aria-labelledby={id}
      className="col-span-4 gap-4 px-6 py-6 max-md:col-span-12 max-md:gap-3 max-md:p-4"
    >
      <h2
        id={id}
        className="text-xs font-bold tracking-[0.08em] text-muted-foreground uppercase"
      >
        {title}
      </h2>
      {children}
    </Card>
  )
}
