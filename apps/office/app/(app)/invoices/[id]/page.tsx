"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"
import { useQueryClient } from "@tanstack/react-query"

import { useSession } from "@genesy/api-client/auth"
import { useInvoicePaymentsVoid } from "@genesy/api-client/endpoints/invoice-payments"
import {
  getInvoicesGetQueryKey,
  getInvoicesListQueryKey,
  useInvoicesGet,
  useInvoicesPreview,
} from "@genesy/api-client/endpoints/invoices"
import { ApiError } from "@genesy/ui/components/api-error"
import { Button } from "@genesy/ui/components/button"
import { ConfirmDialog } from "@genesy/ui/components/confirm-dialog"
import { Notice } from "@genesy/ui/components/notice"
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@genesy/ui/components/table"
import { formatDate } from "@genesy/ui/lib/format"
import { Bento, DetailSkeleton, SubTile, Tile } from "@/components/bento"
import { CardState, PageHead } from "@/components/list"
import { StatusBadge } from "@/components/status-badge"
import { Done, Empty, LoadError, useDone } from "@/components/states"
import { useCan } from "@/lib/can"

import { initials } from "../../orders/shared"
import { invoiceMoney, invoiceNumber } from "../shared"
import { NextStep } from "./actions"

type Text = string | null | undefined

/** One invoice (Template Detail tiles): lines with the API's totals, details + PDF, customer, payments. */
export default function InvoicePage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const { id } = React.use(params)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const invoice = useInvoicesGet(workspaceId, id, {
    query: { enabled: Boolean(workspaceId) },
  })
  // The API renders and stores the PDF on this call, so it runs only on request.
  const [wantPdf, setWantPdf] = React.useState(false)
  const pdf = useInvoicesPreview(workspaceId, id, undefined, {
    query: { enabled: wantPdf, retry: false },
  })
  const queryClient = useQueryClient()
  const changed = () => {
    for (const queryKey of [
      getInvoicesGetQueryKey(workspaceId, id),
      getInvoicesListQueryKey(workspaceId),
    ])
      void queryClient.invalidateQueries({ queryKey })
  }
  const mayVoid = useCan("invoicepayment.delete")
  const voided = useDone()
  const [voiding, setVoiding] = React.useState<string | null>(null)
  const voidPayment = useInvoicePaymentsVoid({
    mutation: {
      onMutate: voided.clear,
      onSuccess: () => {
        setVoiding(null)
        voided.show(t("done.invoice.voided"))
        changed()
      },
    },
  })
  const order = (invoice.data?.data.relationships.order ?? null) as {
    id: string
    number: string
  } | null

  if (invoice.isPending) return <DetailSkeleton />
  if (invoice.isError)
    return (
      <LoadError
        error={invoice.error}
        onRetry={() => void invoice.refetch()}
        backHref="/invoices"
      />
    )
  const inv = invoice.data.data
  const money = (amount: string | number) =>
    invoiceMoney(amount, inv.relationships.currency, locale)
  const nr = invoiceNumber(inv.invoice_nr)
  const title = nr
    ? t(inv.is_credit ? "invoices.creditTitle" : "invoices.detailTitle", {
        number: nr,
      })
    : t(inv.is_credit ? "invoices.draftCreditTitle" : "invoices.draftTitle")
  const payments = inv.relationships.payments ?? []
  // Snapshots on the invoice, shown as stored; any of them may be missing.
  const relation = (inv.relationships.relation ?? {}) as {
    company_name?: Text
    alias?: Text
  }
  const contact = (inv.addressed_contact ?? {}) as {
    first_name?: Text
    last_name?: Text
    email?: Text
  }
  const name =
    relation.company_name ||
    relation.alias ||
    [contact.first_name, contact.last_name].filter(Boolean).join(" ")
  const a = (inv.billing_address ?? {}) as Record<string, Text>
  const address = [
    [a.address, a.house_number, a.house_number_addition]
      .filter(Boolean)
      .join(" "),
    a.address2,
    [a.zipcode, a.city].filter(Boolean).join(" "),
    a.state,
    a.country,
  ].filter(Boolean)

  return (
    <>
      <PageHead
        back={{ href: "/invoices", label: t("invoices.title") }}
        title={title}
        actions={
          <>
            <StatusBadge
              status={inv.status}
              label={t(`invoices.statuses.${inv.status}`)}
            />
            {inv.is_credit ? (
              <StatusBadge status="credit" label={t("invoices.credit")} />
            ) : null}
          </>
        }
      />
      <Bento>
        <NextStep inv={inv} onChanged={changed} />
        <Tile
          id="invoice-lines"
          title={t("invoices.lines")}
          span="col-span-8 row-span-2 max-md:row-span-1"
          flush
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("invoices.item")}</TableHead>
                <TableHead className="text-right">
                  {t("invoices.quantity")}
                </TableHead>
                <TableHead className="text-right">
                  {t("invoices.unitPrice")}
                </TableHead>
                <TableHead className="text-right">
                  {t("invoices.lineTotal")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {inv.items.map((item) => (
                <TableRow key={item.id}>
                  <TableCell className="font-semibold">{item.title}</TableCell>
                  <TableCell data-m="sub" className="text-right tabular-nums">
                    {item.quantity}
                  </TableCell>
                  <TableCell data-m="hide" className="text-right tabular-nums">
                    {money(item.price_incl)}
                  </TableCell>
                  <TableCell
                    data-m="right"
                    className="text-right font-semibold tabular-nums"
                  >
                    {money(item.total_incl)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {/* The API's balance, shown as it comes. */}
          <div className="mt-auto flex flex-col gap-1 border-t border-(--hair) px-5 py-4">
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">
                {t("invoices.total")}
              </span>
              <span className="tabular-nums">{money(inv.balance.total)}</span>
            </p>
            <p className="flex justify-between text-[13px]">
              <span className="text-muted-foreground">
                {t("invoices.paid")}
              </span>
              <span className="tabular-nums">{money(inv.balance.paid)}</span>
            </p>
            <p className="flex items-center justify-between">
              <span className="font-semibold">{t("invoices.open")}</span>
              <span className="text-lg font-semibold tabular-nums max-md:text-base">
                {money(inv.balance.open)}
              </span>
            </p>
            <p className="text-xs text-muted-foreground">
              {t("orders.serverAmounts")}
            </p>
          </div>
        </Tile>

        <Tile
          id="invoice-details"
          title={t("invoices.details")}
          span="col-span-4"
        >
          <dl className="flex flex-col gap-1 text-[13px]">
            <div className="flex justify-between gap-3">
              <dt className="text-muted-foreground">{t("invoices.date")}</dt>
              <dd>{formatDate(inv.created_at, locale)}</dd>
            </div>
            <div className="flex justify-between gap-3">
              <dt className="text-muted-foreground">{t("invoices.due")}</dt>
              <dd>{inv.due_date ? formatDate(inv.due_date, locale) : "—"}</dd>
            </div>
            {order ? (
              <div className="flex items-center justify-between gap-3">
                <dt className="text-muted-foreground">{t("invoices.order")}</dt>
                <dd>
                  <Link
                    className="inline-flex min-h-11 items-center font-mono font-semibold underline-offset-4 hover:underline"
                    href={`/orders/${order.id}`}
                    aria-label={t("invoices.openOrder", {
                      number: order.number,
                    })}
                  >
                    {order.number}
                  </Link>
                </dd>
              </div>
            ) : null}
          </dl>
          {pdf.data ? null : (
            <Button
              variant="outline"
              className="w-full"
              disabled={pdf.isFetching}
              // After a failure the query is already enabled: ask again.
              onClick={() => (wantPdf ? void pdf.refetch() : setWantPdf(true))}
            >
              {t("invoices.makePdf")}
            </Button>
          )}
          <div role="status">
            {pdf.data ? (
              <Notice
                live={false}
                variant="success"
                title={t("done.pdfReady")}
                action={
                  <a
                    className="inline-flex items-center text-sm font-medium underline-offset-4 hover:underline max-md:min-h-11"
                    href={pdf.data.data.file_path}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {t("invoices.openPdf")}
                  </a>
                }
              />
            ) : null}
          </div>
          {(pdf.error as { response?: { status?: number } } | null)?.response
            ?.status === 503 ? (
            <Notice variant="warning" title={t("invoices.pdfUnavailable")} />
          ) : (
            <ApiError error={pdf.error} fallback={t("common.error")} />
          )}
        </Tile>

        <Tile
          id="invoice-customer"
          title={t("invoices.customer")}
          span="col-span-4"
        >
          {name ? (
            <div className="flex items-center gap-3">
              <span
                aria-hidden="true"
                className="inline-flex size-9 shrink-0 items-center justify-center rounded-full bg-[image:var(--gold-gradient)] text-[13px] font-semibold text-[#0a183a]"
              >
                {initials(name)}
              </span>
              <div className="min-w-0">
                <p className="font-semibold break-words">{name}</p>
                {contact.email ? (
                  <p className="text-xs break-words text-muted-foreground">
                    {contact.email}
                  </p>
                ) : null}
              </div>
            </div>
          ) : (
            <p className="text-[13px] text-muted-foreground">—</p>
          )}
          {address.length ? (
            <SubTile className="text-[13px]">
              {address.map((line, i) => (
                <p key={i}>{line}</p>
              ))}
            </SubTile>
          ) : null}
        </Tile>

        <Tile
          id="invoice-payments"
          title={t("invoices.payments")}
          span="col-span-12"
          flush
        >
          {voided.message || voidPayment.error ? (
            <div className="px-5 pt-4">
              <Done message={voided.message} />
              <ApiError
                error={voidPayment.error}
                fallback={t("common.error")}
              />
            </div>
          ) : null}
          {payments.length === 0 ? (
            <CardState>
              <Empty title={t("invoices.noPayments")} />
            </CardState>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("invoices.date")}</TableHead>
                  <TableHead>{t("invoices.method")}</TableHead>
                  <TableHead className="text-right">
                    {t("invoices.amount")}
                  </TableHead>
                  {mayVoid ? (
                    <TableHead>
                      <span className="sr-only">
                        {t("invoices.voidPayment")}
                      </span>
                    </TableHead>
                  ) : null}
                </TableRow>
              </TableHeader>
              <TableBody>
                {payments.map((pay) => (
                  <TableRow key={pay.id}>
                    <TableCell>{formatDate(pay.date, locale)}</TableCell>
                    <TableCell data-m="sub">
                      {t(`payMethods.${pay.payment_method}`)}
                    </TableCell>
                    <TableCell
                      data-m="right"
                      className="text-right font-semibold tabular-nums"
                    >
                      {money(pay.amount)}
                    </TableCell>
                    {mayVoid ? (
                      <TableCell className="text-right">
                        <Button
                          variant="outline"
                          size="sm"
                          className="max-md:min-h-11"
                          onClick={() => setVoiding(pay.id)}
                        >
                          {t("invoices.voidPayment")}
                        </Button>
                      </TableCell>
                    ) : null}
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </Tile>
      </Bento>
      <ConfirmDialog
        open={voiding !== null}
        onOpenChange={(open) => (open ? null : setVoiding(null))}
        title={t("invoices.voidTitle")}
        description={t("invoices.voidText")}
        confirm={t("invoices.voidPayment")}
        cancel={t("actions.cancel")}
        disabled={voidPayment.isPending}
        onConfirm={() =>
          voiding &&
          voidPayment.mutate({
            workspace: workspaceId,
            invoice: id,
            payment: voiding,
          })
        }
      />
    </>
  )
}
