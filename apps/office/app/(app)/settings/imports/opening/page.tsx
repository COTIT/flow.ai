"use client"

import * as React from "react"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getAccountingSetupGetQueryKey,
  useAccountingOpeningValidate,
} from "@genesy/api-client/endpoints/accounting"
import { useImportsList } from "@genesy/api-client/endpoints/imports"
import type { CurrencyRef } from "@genesy/api-client/model"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import { formatDateTime, formatQty } from "@genesy/ui/lib/format"
import { useQueryClient } from "@tanstack/react-query"
import { Bento, Metric, Tile } from "@/components/bento"
import { Field, selectClass } from "@/components/field"
import { PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { LoadError, NoAccess } from "@/components/states"
import { useCan } from "@/lib/can"
import { OPENING, type Batch } from "@/lib/imports/flow"

import { useBookMonth, useReasons } from "../../../accounting/shared"
import { showMoney } from "../../../purchases/shared"

type Sums = Record<string, string | number | null>
type Review = {
  setup_version: number
  ready: boolean
  issues: { code: string; message: string }[]
  opening_fingerprint: string
  inventory: Sums
  receivables: Sums
  payables: Sums
  trial_balance: Sums
}

/** The finished imports of one opening profile, newest first (one page). */
function useCompleted(profile: string) {
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const list = useImportsList(
    workspaceId,
    { profile, page_size: "20" },
    { query: { enabled: Boolean(workspaceId) } }
  )
  const done = ((list.data?.data ?? []) as Batch[]).filter(
    (b) => b.state === "completed"
  )
  return { list, done }
}

/**
 * Opening review (Detail tiles): pick at most one finished import per opening profile and let
 * the API check them together (no writes). Activation is a later step; there is no button here.
 */
export default function OpeningReviewPage() {
  const t = useTranslations()
  const locale = useLocale()
  const may = useCan("accounting.configure")
  const reasons = useReasons()
  const queryClient = useQueryClient()
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const book = useBookMonth()
  const lists = {
    opening_inventory: useCompleted("opening_inventory"),
    opening_receivables: useCompleted("opening_receivables"),
    opening_payables: useCompleted("opening_payables"),
    opening_trial_balance: useCompleted("opening_trial_balance"),
  }
  const [chosen, setChosen] = React.useState<Record<string, string>>({})
  const pick = (p: (typeof OPENING)[number]) =>
    chosen[p] ?? lists[p].done[0]?.id ?? ""
  const review = useAccountingOpeningValidate({
    mutation: {
      onError: () =>
        void queryClient.invalidateQueries({
          queryKey: getAccountingSetupGetQueryKey(workspaceId),
        }),
    },
  })
  const r = review.data?.data as Review | undefined
  const currency = book.setup.data?.data.functional_currency ?? undefined
  const money = (v: string | number | null | undefined) =>
    v == null
      ? "–"
      : currency
        ? showMoney(String(v), currency as CurrencyRef, locale)
        : String(v)
  const ids = OPENING.map(pick).filter(Boolean)
  const failed = Object.values(lists).find((l) => l.list.isError)?.list

  if (!may) return <NoAccess />
  if (book.setup.isError || failed)
    return (
      <LoadError
        error={book.setup.error ?? failed?.error}
        onRetry={() => {
          void book.setup.refetch()
          void failed?.refetch()
        }}
        backHref="/settings/imports"
      />
    )

  return (
    <>
      <PageHead
        eyebrow={t("shell.settings")}
        title={t("opening.title")}
        description={t("opening.lead")}
        back={{ href: "/settings/imports", label: t("imports.back") }}
      />
      <Notice live={false} title={t("opening.later")} />
      <Bento>
        <Tile id="op-pick" title={t("opening.batches")} span="col-span-12">
          <form
            className="grid gap-4 md:grid-cols-2"
            onSubmit={(e) => {
              e.preventDefault()
              const setup = book.setup.data?.data
              if (!setup) return
              review.mutate({
                workspace: workspaceId,
                data: {
                  batch_ids: ids,
                  expected_setup_version: setup.version,
                },
              })
            }}
          >
            {OPENING.map((p) => (
              <Field key={p} id={`op-${p}`} label={t(`imports.profiles.${p}`)}>
                <select
                  className={selectClass}
                  value={pick(p)}
                  onChange={(e) => {
                    setChosen((c) => ({ ...c, [p]: e.target.value }))
                    review.reset()
                  }}
                >
                  <option value="">{t("opening.none")}</option>
                  {lists[p].done.map((b) => (
                    <option key={b.id} value={b.id}>
                      {`${b.filename} · ${formatDateTime(b.created_at, locale)}`}
                    </option>
                  ))}
                </select>
              </Field>
            ))}
            <div className="flex flex-col gap-3 md:col-span-2">
              {ids.length === 0 ? (
                <Notice variant="info" title={t("opening.nothing")} />
              ) : null}
              <ApiError
                error={review.error}
                fallback={t("common.error")}
                codes={reasons}
              />
              <Button
                type="submit"
                className="min-h-11 self-start max-md:w-full"
                disabled={
                  ids.length === 0 || review.isPending || !book.setup.data
                }
              >
                {review.isPending
                  ? t("opening.reviewing")
                  : t("opening.review")}
              </Button>
            </div>
          </form>
        </Tile>
        {r ? (
          <>
            <Tile
              id="op-result"
              title={t("opening.result")}
              span="col-span-12"
              action={
                <StatusBadge
                  status={r.ready ? "ready" : "issues"}
                  label={t(r.ready ? "opening.ready" : "opening.notReady")}
                  tone={r.ready ? "teal" : "red"}
                />
              }
            >
              {r.issues.length ? (
                <ul
                  className="flex flex-col gap-2"
                  aria-label={t("opening.issues")}
                >
                  {r.issues.map((i) => (
                    <li key={i.code}>
                      <Notice variant="warning" title={i.message} />
                    </li>
                  ))}
                </ul>
              ) : (
                <Notice variant="success" title={t("opening.allGood")} />
              )}
              <p className="text-[13px] break-all text-muted-foreground">
                {t("opening.fingerprint", {
                  value: r.opening_fingerprint.slice(0, 12),
                })}
              </p>
            </Tile>
            <Tile id="op-inv" title={t("opening.inventory")} span="col-span-6">
              <div className="grid grid-cols-2 gap-3 max-md:grid-cols-1">
                <Metric label={t("opening.mode")}>
                  {r.inventory.mode
                    ? t(`imports.modes.${r.inventory.mode}`)
                    : "–"}
                </Metric>
                <Metric label={t("opening.quantity")}>
                  {formatQty(String(r.inventory.quantity ?? "0"), locale)}
                </Metric>
                <Metric label={t("opening.value")}>
                  {money(r.inventory.value)}
                </Metric>
              </div>
            </Tile>
            <Tile
              id="op-tb"
              title={t("opening.trialBalance")}
              span="col-span-6"
            >
              <div className="grid grid-cols-2 gap-3 max-md:grid-cols-1">
                <Metric label={t("opening.debit")}>
                  {money(r.trial_balance.debit)}
                </Metric>
                <Metric label={t("opening.credit")}>
                  {money(r.trial_balance.credit)}
                </Metric>
              </div>
            </Tile>
            {(["receivables", "payables"] as const).map((k) => (
              <Tile
                key={k}
                id={`op-${k}`}
                title={t(`opening.${k}`)}
                span="col-span-6"
              >
                <div className="grid grid-cols-2 gap-3 max-md:grid-cols-1">
                  <Metric label={t("opening.count")}>
                    {String(r[k].count ?? 0)}
                  </Metric>
                  <Metric label={t("opening.face")}>{money(r[k].face)}</Metric>
                  <Metric label={t("opening.settled")}>
                    {money(r[k].settled)}
                  </Metric>
                  <Metric label={t("opening.outstanding")}>
                    {money(r[k].outstanding)}
                  </Metric>
                  <Metric label={t("opening.carrying")}>
                    {money(r[k].carrying)}
                  </Metric>
                </div>
              </Tile>
            ))}
          </>
        ) : null}
      </Bento>
    </>
  )
}
