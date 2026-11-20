"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@verzo/api-client/auth"
import {
  getSupplierPaymentsGetQueryKey,
  getSupplierPaymentsListQueryKey,
  getSupplierPaymentsTargetsQueryKey,
  supplierPaymentsAllocate,
  supplierPaymentsAllocateReview,
  supplierPaymentsReverse,
  supplierPaymentsReverseReview,
  useSupplierPaymentsGet,
  useSupplierPaymentsTargets,
} from "@verzo/api-client/endpoints/supplier-payments"
import type { SupplierPaymentResource } from "@verzo/api-client/model"
import { Button } from "@verzo/ui/components/button"
import { Input } from "@verzo/ui/components/input"
import { Label } from "@verzo/ui/components/label"
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
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { Field } from "@/components/field"
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
import { bookAmount, showMoney, showRate } from "../../purchases/shared"
import type { Outcome } from "../../purchases/[id]/dialogs"

/** A supplier payment or refund received: facts, unapplied amount, allocations, next step. */
export default function SupplierPaymentPage({
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
  const payment = useSupplierPaymentsGet(workspaceId, id, { query: { enabled: Boolean(workspaceId) } })
  const done = useDone(arrived === "created" ? t("payables.done.recorded") : null)
  const [stale, setStale] = React.useState(false)
  const refresh = () => {
    for (const queryKey of [
      getSupplierPaymentsGetQueryKey(workspaceId, id),
      getSupplierPaymentsListQueryKey(workspaceId),
      getSupplierPaymentsTargetsQueryKey(workspaceId, id),
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
  if (payment.isPending) return <DetailSkeleton />
  if (payment.isError)
    return <LoadError error={payment.error} onRetry={() => void payment.refetch()} backHref="/supplier-payments" />
  const p = payment.data.data
  const money = (amount: string) => showMoney(amount, p.currency, locale)
  const allowed = (["allocate", "reverse"] as const).filter(
    (a) => p.allowed_actions.includes(a) && rights.may(`supplierpayment.${a}`)
  )
  const [main, ...rest] = allowed
  const actionButton = (a: (typeof allowed)[number], big: boolean) => {
    const button = big ? <Button size="lg" className="w-full" /> : <Button variant="outline" className="w-full" />
    return a === "allocate" ? (
      <AllocateDialog key={a} payment={p} button={button} outcome={outcome} />
    ) : (
      <ReversePaymentDialog key={a} payment={p} button={button} outcome={outcome} />
    )
  }

  return (
    <>
      <PageHead
        back={{ href: "/supplier-payments", label: t("payables.payments.title") }}
        title={t("payables.payments.detailTitle", { number: p.number })}
        description={`${p.supplier.name ?? ""} · ${word("directions", p.direction)}`}
        actions={<StatusBadge status={p.state} label={word("states", p.state)} />}
      />
      {stale ? <Notice variant="warning" title={t("payables.stale")} /> : null}
      <Bento>
        <Tile id="pay-allocations" title={t("payables.payments.allocations")} span="col-span-8 row-span-2 max-md:row-span-1" flush>
          {p.allocations.length === 0 ? (
            <p className="px-5 py-4 text-[13px] text-muted-foreground">{t("payables.payments.noAllocations")}</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("payables.accountingDate")}</TableHead>
                  <TableHead>{t("payables.state")}</TableHead>
                  <TableHead className="text-right">{t("payables.payments.fx")}</TableHead>
                  <TableHead className="text-right">{t("payables.amount")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {p.allocations.map((a) => (
                  <TableRow key={a.allocation_id}>
                    <TableCell>{formatDay(a.accounting_date, locale)}</TableCell>
                    <TableCell data-m="badge">
                      <StatusBadge
                        status={a.reversed ? "reversed" : "posted"}
                        label={word("states", a.reversed ? "reversed" : "posted")}
                      />
                    </TableCell>
                    <TableCell data-m="hide" className="text-right tabular-nums">
                      {a.fx ? bookAmount(a.fx, locale) : ""}
                    </TableCell>
                    <TableCell data-m="right" className="text-right font-semibold tabular-nums">
                      {money(a.amount)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
          <div className="mt-auto flex flex-col gap-1 border-t border-(--hair) px-5 py-4 text-[13px]">
            <Fact label={t("payables.amount")} value={money(p.amount)} />
            <p className="flex items-center justify-between">
              <span className="font-semibold">{t("payables.payments.unapplied")}</span>
              <span className="text-lg font-semibold tabular-nums max-md:text-base">{money(p.unapplied)}</span>
            </p>
          </div>
        </Tile>

        <SideCard id="pay-next" title={t("payables.nextStep")}>
          <Done message={done.message} />
          {p.state === "reversed" ? (
            <Notice
              title={t("payables.reversedInfo", {
                date: p.reversal_date ? formatDay(p.reversal_date, locale) : "",
                reason: p.reverse_reason ?? "",
              })}
            />
          ) : allowed.length === 0 ? (
            <Notice title={t("payables.nothingToDo")} />
          ) : null}
          {main ? (
            <>
              <p className="text-[13px]">{t(`payables.payments.next.${main}`)}</p>
              {actionButton(main, true)}
            </>
          ) : null}
          {rest.map((a) => actionButton(a, false))}
        </SideCard>

        <SideCard id="pay-facts" title={t("payables.facts")}>
          <p className="font-semibold break-words">{p.supplier.name ?? ""}</p>
          <SubTile className="flex flex-col gap-1 text-[13px]">
            <Fact label={t("payables.payments.date")} value={formatDay(p.date, locale)} />
            <Fact label={t("payables.payments.place")} value={word("locationKinds", p.location_kind)} />
            {p.reference ? <Fact label={t("payables.payments.reference")} value={p.reference} /> : null}
            <Fact label={t("payables.evidenceRef")} value={p.evidence_ref} />
            <Fact label={t("payables.rate")} value={showRate(p.rate.rate, locale)} />
            {p.functional_amount ? (
              <Fact label={t("payables.functionalTotal")} value={bookAmount(p.functional_amount, locale)} />
            ) : null}
          </SubTile>
          {p.note ? <p className="text-[13px] break-words">{p.note}</p> : null}
          <Link className="text-[13px] underline-offset-4 hover:underline" href={`/supplier-bills?supplier=${p.supplier.id}`}>
            {t("payables.payments.supplierBills")}
          </Link>
        </SideCard>
      </Bento>
    </>
  )
}

type Props = { payment: SupplierPaymentResource; button: React.ReactElement; outcome: Outcome }

/**
 * Allocate: the open items the server says this record may settle (a payment's bills, a
 * refund's credits), one chosen with an amount, the server's review (FX), then the allocation.
 */
function AllocateDialog({ payment, button, outcome }: Props) {
  const t = useTranslations()
  const locale = useLocale()
  const word = usePayWord()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const [open, setOpen] = React.useState(false)
  const targets = useSupplierPaymentsTargets(ws, payment.id, { page_size: "50" }, {
    query: { enabled: open && Boolean(ws) },
  })
  const [target, setTarget] = React.useState("")
  const [amount, setAmount] = React.useState("")
  const money = (a: string) => showMoney(a, payment.currency, locale)
  const rows = targets.data?.data ?? []
  return (
    <ReviewDialog
      button={button}
      outcome={outcome}
      trigger={t("payables.payments.allocate")}
      title={t("payables.payments.allocateTitle", { number: payment.number })}
      description={t("payables.payments.allocateText", { unapplied: money(payment.unapplied) })}
      done={t("payables.done.allocated")}
      onOpen={() => {
        setOpen(true)
        setTarget("")
        setAmount("")
      }}
      ready={Boolean(target && amount.trim())}
      fields={(errors) => (
        <>
          {targets.isPending ? (
            <p className="text-[13px] text-muted-foreground">{t("payables.reviewing")}</p>
          ) : targets.isError ? (
            <LoadError error={targets.error} onRetry={() => void targets.refetch()} />
          ) : rows.length === 0 ? (
            <Notice title={t("payables.payments.noTargets")} />
          ) : (
            <div role="radiogroup" aria-label={t("payables.payments.target")} className="flex max-h-[40vh] flex-col gap-1 overflow-y-auto">
              {rows.map((r) => (
                <Label key={r.open_item_id} className="flex min-h-11 items-center gap-3 font-normal">
                  <input
                    type="radio"
                    name="target"
                    className="size-4"
                    checked={target === r.open_item_id}
                    onChange={() => {
                      setTarget(r.open_item_id)
                      setAmount(r.residual)
                    }}
                  />
                  <span className="flex-1">
                    {word("sourceKinds", r.source_kind)}
                    {r.source_number != null ? ` ${r.source_number}` : ""}
                    {r.supplier_invoice_number ? ` · ${r.supplier_invoice_number}` : ""}
                    <span className="block text-xs text-muted-foreground">
                      {r.due_date ? t("payables.due", { date: formatDay(r.due_date, locale) }) : formatDay(r.issue_date, locale)}
                    </span>
                  </span>
                  <span className="tabular-nums">{money(r.residual)}</span>
                </Label>
              ))}
            </div>
          )}
          <Field
            id="alloc-amount"
            label={t("payables.amount")}
            errors={errors["payload.amount"] ?? errors["amount"] ?? errors["payload.target_open_item_id"]}
          >
            <Input inputMode="decimal" required value={amount} onChange={(e) => setAmount(e.target.value)} />
          </Field>
        </>
      )}
      review={async () =>
        (await supplierPaymentsAllocateReview(ws, payment.id, { target_open_item_id: target, amount: amount.trim() })).data
      }
      renderReview={(r) => (
        <>
          <Fact label={t("payables.amount")} value={money(r.amount)} />
          {r.fx ? <Fact label={t("payables.payments.fx")} value={bookAmount(r.fx, locale)} /> : null}
        </>
      )}
      commit={(fp, client_ref, request) =>
        supplierPaymentsAllocate(
          ws,
          payment.id,
          {
            client_ref,
            expected_version: payment.version,
            payload: { target_open_item_id: target, amount: amount.trim(), review_fingerprint: fp },
          },
          request
        )
      }
    />
  )
}

/** Reverse an unapplied, wrongly recorded payment: date + reason, review, reversal. */
function ReversePaymentDialog({ payment, button, outcome }: Props) {
  const t = useTranslations()
  const bookToday = useBookToday()
  const locale = useLocale()
  const ws = useSession((s) => s.workspaceId) ?? ""
  const inputs = useReverseInputs("pay-rev")
  return (
    <ReviewDialog
      button={button}
      outcome={outcome}
      trigger={t("payables.reverse")}
      title={t("payables.payments.reverseTitle", { number: payment.number })}
      description={t("payables.payments.reverseText")}
      done={t("payables.done.reversed")}
      onOpen={() => inputs.reset(bookToday)}
      ready={inputs.ready}
      fields={inputs.fields}
      review={async () => (await supplierPaymentsReverseReview(ws, payment.id, inputs.body)).data}
      renderReview={(r) => (
        <>
          <Fact label={t("payables.accountingDate")} value={formatDay(r.accounting_date, locale)} />
          <Fact label={t("payables.reason")} value={r.reason} />
        </>
      )}
      commit={(fp, client_ref, request) =>
        supplierPaymentsReverse(
          ws,
          payment.id,
          { client_ref, expected_version: payment.version, payload: { ...inputs.body, review_fingerprint: fp } },
          request
        )
      }
    />
  )
}
