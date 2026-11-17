"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useTaxProfilesList } from "@genesy/api-client/endpoints/tax-profiles"
import { buttonVariants } from "@genesy/ui/components/button"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDay } from "@genesy/ui/lib/format"
import { selectClass } from "@/components/field"
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

import { showRate } from "../../purchases/shared"
import { placeName, useWord } from "./shared"

const states = ["draft", "verified"] as const

/** Tax profiles (Template List): state filter, effective dates, revision; add a draft. */
export default function TaxesPage({
  searchParams,
}: {
  searchParams: Promise<{ state?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const router = useRouter()
  const word = useWord()
  const mayCreate = useCan("taxprofile.configure")
  const { state, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const profiles = useTaxProfilesList(
    workspaceId,
    { state: state || undefined, page: page ? Number(page) : undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const go = (next: { state?: string; page?: number }) => {
    const params = new URLSearchParams()
    const st = next.state ?? state
    if (st) params.set("state", st)
    if (next.page && next.page > 1) params.set("page", String(next.page))
    router.push(`?${params}`)
  }
  const columns = [
    { label: t("taxes.profile") },
    { label: t("taxes.rates") },
    { label: t("taxes.state") },
    { label: t("taxes.effective") },
  ]

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("taxes.title")}
        description={t("taxes.hint")}
        actions={
          mayCreate ? (
            <Link href="/settings/taxes/new" className={buttonVariants()}>
              {t("taxes.new")}
            </Link>
          ) : null
        }
      />
      <ListCard
        filters={
          <select
            aria-label={t("taxes.state")}
            className={`${selectClass} w-[200px] max-md:w-full`}
            value={state ?? ""}
            onChange={(e) => go({ state: e.target.value })}
          >
            <option value="">{t("taxes.allStates")}</option>
            {states.map((s) => (
              <option key={s} value={s}>
                {word("states", s)}
              </option>
            ))}
          </select>
        }
        count={
          profiles.data
            ? t("taxes.count", { total: profiles.data.meta.total })
            : null
        }
        pager={
          profiles.data?.data.length ? (
            <ListPager
              meta={profiles.data.meta}
              onPage={(p) => go({ page: p })}
            />
          ) : null
        }
      >
        {profiles.isPending ? (
          <ListSkeleton columns={columns} />
        ) : profiles.isError ? (
          <CardState>
            <LoadError
              error={profiles.error}
              onRetry={() => void profiles.refetch()}
            />
          </CardState>
        ) : profiles.data.data.length === 0 ? (
          <CardState>
            <Empty
              title={t("taxes.empty")}
              onClear={state ? () => router.push("?") : undefined}
            />
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
              {profiles.data.data.map((p) => (
                <TableRow key={p.id}>
                  <TableCell>
                    <Link
                      className="font-semibold underline-offset-4 hover:underline"
                      href={`/settings/taxes/${p.id}`}
                    >
                      {t("taxes.profileTitle", {
                        place: placeName(p, locale),
                        revision: p.revision,
                      })}
                    </Link>
                    <span className="block text-xs text-muted-foreground">
                      {p.transaction_types
                        .map((x) => word("transactionTypes", x))
                        .join(", ")}
                    </span>
                  </TableCell>
                  <TableCell data-m="sub" className="tabular-nums">
                    {p.components
                      .map((c) => `${c.name} ${showRate(c.rate, locale)}%`)
                      .join(" · ")}
                  </TableCell>
                  <TableCell data-m="badge">
                    <StatusBadge
                      status={p.superseded_by_id ? "superseded" : p.state}
                      label={word(
                        "states",
                        p.superseded_by_id ? "superseded" : p.state
                      )}
                      tone={
                        p.superseded_by_id
                          ? "slate"
                          : p.state === "verified"
                            ? "teal"
                            : "gold"
                      }
                    />
                  </TableCell>
                  <TableCell data-m="hide" className="text-muted-foreground">
                    {p.effective_to
                      ? t("taxes.between", {
                          from: formatDay(p.effective_from, locale),
                          to: formatDay(p.effective_to, locale),
                        })
                      : t("taxes.since", {
                          from: formatDay(p.effective_from, locale),
                        })}
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
