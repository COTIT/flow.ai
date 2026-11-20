"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  getPurchaseReturnsGetQueryKey,
  getPurchaseReturnsListQueryKey,
  purchaseReturnsPost,
  purchaseReturnsPostReview,
  purchaseReturnsReverse,
  purchaseReturnsReverseReview,
  usePurchaseReturnsGet,
} from "@genesy/api-client/endpoints/purchase-returns"
import type { PurchaseReturnResource } from "@genesy/api-client/model"
import { Button } from "@genesy/ui/components/button"
import { Notice } from "@genesy/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDay, formatQty } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { PageHead } from "@/components/list"
import {
  Fact,
  ReviewDialog,
  SideCard,
  usePayWord,
  useReverseInputs,
} from "@/components/payables"
import { StatusBadge } from "@/components/status-badge"
import { Done, LoadError, useDone } from "@/components/states"
import { useRights } from "@/lib/can"

import { useBookToday } from "../../accounting/shared"
import { bookAmount, showMoney } from "../../purchases/shared"
import type { Outcome } from "../../purchases/[id]/dialogs"

/** A purchase return: lines (goods out, credit), next step (post, reverse), facts. */
export default function PurchaseReturnPage({
  params,
  searchParams,
}: {
  params: Promise<{ id: string }>
  searchParams: Promise<{ done?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = usePayWord()
  const { id } = React.use(params)
  const { done: arrived } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const queryClient = useQueryClient()
  const rights = useRights()
  const ret = usePurchaseReturnsGet(workspaceId, id, { query: { enabled: Boolean(workspaceId) } })
  const done = useDone(arrived === "created" ? t("payables.done.created") : null)
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [getPurchaseReturnsGetQueryKey(workspaceId, id), getPurchaseReturnsListQueryKey(workspaceId)])
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
  if (ret.isPending) return <DetailSkeleton />
  if (ret.isError)
    return <LoadError error={ret.error} onRetry={() => void ret.refetch()} backHref="/purchase-returns" />
  const r = ret.data.data
  const allowed = (["post", "reverse"] as const).filter(
    (a) => r.allowed_actions.includes(a) && rights.may(`purchasereturn.${a}`)
  )
  const main = allowed[0]
  const fn = (v?: string | null) => (v == null ? "" : bookAmount(v, locale))

  return (
    <>
      <PageHead
        back={{ href: "/purchase-returns", label: t("payables.returns.title") }}
        title={t("payables.returns.detailTitle", { number: r.number })}
        description={`${r.supplier.name ?? ""} · ${word("purposes", r.purpose)}`}
        actions={<StatusBadge status={r.state} label={word("states", r.state)} />}
      />
      {stale ? <Notice variant="warning" title={t("payables.stale")} /> : null}
      <Bento>
        <Tile id="ret-lines" title={t("payables.lines")} span="col-span-8 row-span-2 max-md:row-span-1" flush>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("payables.product")}</TableHead>
                <TableHead className="text-right">{t("payables.quantity")}</TableHead>
                <TableHead className="text-right">{t("payables.returns.issueValue")}</TableHead>
                <TableHead className="text-right">{t("payables.returns.credit")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {r.lines.map((l) => (
                <TableRow key={l.id}>
                  <TableCell>
                    <span className="block font-semibold">{l.product_name}</span>
                    <span className="block text-xs text-muted-foreground">
                      {l.warehouse_name} · {l.billed ? t("payables.returns.billedLine") : t("payables.returns.unbilledLine")}
                    </span>
                  </TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {formatQty(l.quantity, locale)}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {fn(l.issue_value)}
                  </TableCell>
                  <TableCell data-m="right" className="text-right font-semibold tabular-nums">
                    {l.credit_nominal ? showMoney(l.credit_nominal, r.currency, locale) : ""}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <p className="mt-auto border-t border-(--hair) px-5 py-4 text-xs text-muted-foreground">
            {r.state === "draft" ? t("payables.returns.valuesAtPost") : t("payables.serverAmounts")}
          </p>
        </Tile>

        <SideCard id="ret-next" title={t("payables.nextStep")}>
          <Done message={done.message} />
          {r.state === "reversed" ? (
            <Notice title={t("payables.reversedInfo", { date: "", reason: r.reverse_reason ?? "" })} />
          ) : !main ? (
            <Notice title={t("payables.nothingToDo")} />
          ) : (
            <>
              <p className="text-[13px]">{t(`payables.returns.next.${main}`)}</p>
              {main === "post" ? (
                <PostReturnDialog ret={r} button={<Button size="lg" className="w-full" />} outcome={outcome} />
              ) : (
                <ReverseReturnDialog ret={r} button={<Button variant="outline" className="w-full" />} outcome={outcome} />
              )}
            </>
          )}
        </SideCard>

        <SideCard id="ret-facts" title={t("payables.facts")}>
          <p className="font-semibold break-words">{r.supplier.name ?? ""}</p>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
            <Fact label={t("payables.reason")} value={r.reason} />
            <Fact label={t("payables.currency")} value={r.currency.code} />
            <Fact
              label={t("payables.returns.applyCreditShort")}
              value={r.apply_credit ? t("payables.yes") : t("payables.no")}
            />
            {r.evidence_ref ? <Fact label={t("payables.evidenceRef")} value={r.evidence_ref} /> : null}
          </SubTile>
        </SideCard>
      </Bento>
    </>
  )
}

type Props = { ret: PurchaseReturnResource; button: React.ReactElement; outcome: Outcome }

/** Post: the server's review (stock value out, GRNI, credit, variance) then its fingerprint. */
function PostReturnDialog({ ret, button, outcome }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const name = (line: string) => ret.lines.find((l) => l.id === line)?.product_name ?? ""
  return (
    <ReviewDialog
      auto
      button={button}
      outcome={outcome}
      trigger={t("payables.returns.post")}
      title={t("payables.returns.postTitle", { number: ret.number })}
      description={t("payables.returns.postText")}
      done={t("payables.done.posted")}
      review={async () => (await purchaseReturnsPostReview(ws, ret.id)).data}
      renderReview={(r) => (
        <>
          <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
          <ul className="flex flex-col gap-1">
            {r.lines.map((l) => (
              <li key={l.line_id} className="flex flex-wrap justify-between gap-x-3">
                <span>
                  {name(l.line_id)} × {formatQty(l.quantity, locale)}
                </span>
                <span className="tabular-nums">
                  {l.credit_nominal
                    ? `${t("payables.returns.credit")} ${showMoney(l.credit_nominal, ret.currency, locale)}`
                    : null}
                  {l.issue_value ? ` · ${t("payables.returns.issueValue")} ${bookAmount(l.issue_value, locale)}` : null}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      commit={(fp, client_ref, request) =>
        purchaseReturnsPost(
          ws,
          ret.id,
          { client_ref, expected_version: ret.version, payload: { review_fingerprint: fp } },
          request
        )
      }
    />
  )
}

/** Reverse: goods back at their original value and the credit closed, both together. */
function ReverseReturnDialog({ ret, button, outcome }: Props) {
  const t = useTranslations()
  const bookToday = useBookToday()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const inputs = useReverseInputs("ret-rev")
  // Both sides together: reversing one alone is refused by the API (not supported yet).
  const body = () => ({ ...inputs.body, reverse_financial: true, reverse_physical: true })
  return (
    <ReviewDialog
      button={button}
      outcome={outcome}
      trigger={t("payables.reverse")}
      title={t("payables.returns.reverseTitle", { number: ret.number })}
      description={t("payables.returns.reverseText")}
      done={t("payables.done.reversed")}
      onOpen={() => inputs.reset(bookToday)}
      ready={inputs.ready}
      fields={inputs.fields}
      review={async () => (await purchaseReturnsReverseReview(ws, ret.id, body())).data}
      renderReview={(r) => (
        <>
          <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
          <Fact label={t("payables.reason")} value={r.reason} />
        </>
      )}
      commit={(fp, client_ref, request) =>
        purchaseReturnsReverse(
          ws,
          ret.id,
          { client_ref, expected_version: ret.version, payload: { ...body(), review_fingerprint: fp } },
          request
        )
      }
    />
  )
}
