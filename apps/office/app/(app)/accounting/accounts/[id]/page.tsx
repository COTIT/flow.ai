"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useAccountingAccountsGet } from "@genesy/api-client/endpoints/accounting"
import { Button } from "@genesy/ui/components/button"
import { Input } from "@genesy/ui/components/input"
import { Field } from "@/components/field"
import { ListCard, ListSkeleton, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { LoadError } from "@/components/states"

import { Ledger } from "../../ledger"
import { useBookMonth, useWord } from "../../shared"

type Query = { from?: string; to?: string; page?: string }

/**
 * One account (Template List): its facts and its general-ledger history, a date range and
 * pages. Figures are the report route's strings; nothing is added up here.
 */
export default function AccountPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  searchParams: Promise<Query>
}) {
  const t = useTranslations()
  const router = useRouter()
  const word = useWord()
  const { id } = React.use(params)
  const q = React.use(searchParams)
  const book = useBookMonth()
  const from = q.from ?? book.month?.from ?? ""
  const to = q.to ?? book.month?.to ?? ""
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const on = (more: boolean) => ({
    query: { enabled: Boolean(workspaceId) && more },
  })
  const one = useAccountingAccountsGet(workspaceId, id, on(true))
  const account = one.data?.data
  const parentId = account?.parent_id ?? ""
  const parent = useAccountingAccountsGet(workspaceId, parentId, on(!!parentId))
    .data?.data
  const go = (next: Query) => {
    const params = new URLSearchParams()
    for (const [k, v] of Object.entries({ ...q, ...next }))
      if (v) params.set(k, v)
    router.push(`?${params}`)
  }
  const back = {
    href: "/accounting/accounts",
    label: t("accounting.accounts.title"),
  }

  const failed = one.isError ? one : book.setup.isError ? book.setup : null
  if (failed)
    return (
      <LoadError
        error={failed.error}
        onRetry={() => void failed.refetch()}
        backHref={back.href}
      />
    )

  return (
    <>
      <PageHead
        back={back}
        title={
          account
            ? `${account.code} ${account.name}`
            : t("accounting.accounts.history")
        }
        description={
          account
            ? [
                word("types", account.type),
                parent
                  ? t("accounting.accounts.under", {
                      code: parent.code,
                      name: parent.name,
                    })
                  : null,
                account.roles.length
                  ? account.roles.map((r) => word("roles", r)).join(", ")
                  : null,
              ]
                .filter(Boolean)
                .join(" · ")
            : undefined
        }
        actions={
          account ? (
            <StatusBadge
              status={account.active ? "active" : "inactive"}
              label={t(
                account.active
                  ? "accounting.accounts.active"
                  : "accounting.accounts.inactive"
              )}
              tone={account.active ? "teal" : "slate"}
            />
          ) : null
        }
      />
      <ListCard
        filters={
          <form
            key={`${from}|${to}`}
            className="flex flex-wrap items-end gap-3 max-md:w-full max-md:flex-col max-md:items-stretch"
            onSubmit={(e) => {
              e.preventDefault()
              const f = new FormData(e.currentTarget)
              go({
                from: String(f.get("from") ?? "") || undefined,
                to: String(f.get("to") ?? "") || undefined,
                page: undefined,
              })
            }}
          >
            <Field id="ah-from" label={t("accounting.reports.from")}>
              <Input type="date" name="from" required defaultValue={from} />
            </Field>
            <Field id="ah-to" label={t("accounting.reports.to")}>
              <Input type="date" name="to" required defaultValue={to} />
            </Field>
            <Button type="submit">{t("accounting.reports.show")}</Button>
          </form>
        }
      >
        {from && to && account ? (
          <Ledger
            account={id}
            from={from}
            to={to}
            page={q.page}
            onPage={(p) => go({ page: String(p) })}
          />
        ) : (
          <ListSkeleton columns={[{ label: "" }, { label: "", right: true }]} />
        )}
      </ListCard>
    </>
  )
}
