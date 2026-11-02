"use client"

import * as React from "react"
import { useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import {
  useInvoicesGet,
  useInvoicesPreview,
} from "@genesy/api-client/endpoints/invoices"
import { ApiError } from "@genesy/ui/components/api-error"
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
import { StatusBadge } from "@/components/status-badge"
import { Empty, LoadError } from "@/components/states"

export default function InvoicePage({
  params,
}: {
  params: Promise<{ id: string }>
}) {
  const t = useTranslations()
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

  if (invoice.isPending)
    return (
      <p className="text-sm text-muted-foreground">{t("common.loading")}</p>
    )
  if (invoice.isError)
    return (
      <LoadError
        error={invoice.error}
        onRetry={() => void invoice.refetch()}
        backHref="/invoices"
      />
    )
  const inv = invoice.data.data
  const payments = inv.relationships.payments ?? []

  return (
    <>
      <section className="flex flex-col gap-3">
        <div className="flex flex-wrap items-center gap-2">
          <h1 className="heading-2 text-lg">
            {inv.invoice_nr ?? t("invoices.draft")}
          </h1>
          <StatusBadge status={inv.status} />
          {inv.is_credit ? (
            <StatusBadge status="credit" label={t("invoices.credit")} />
          ) : null}
          <div className="ml-auto flex items-center gap-2">
            {pdf.data ? null : (
              <Button
                variant="outline"
                disabled={pdf.isFetching}
                onClick={() => setWantPdf(true)}
              >
                {t("invoices.makePdf")}
              </Button>
            )}
          </div>
        </div>
        <div role="status">
          {pdf.data ? (
            <Notice
              live={false}
              variant="success"
              title={t("done.pdfReady")}
              action={
                <a
                  className="text-sm font-medium underline-offset-4 hover:underline"
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
        <ApiError error={pdf.error} fallback={t("common.error")} />
        <dl className="grid max-w-md grid-cols-2 gap-x-4 gap-y-1 text-sm">
          <dt className="text-muted-foreground">{t("invoices.date")}</dt>
          <dd>{inv.created_at.slice(0, 10)}</dd>
          <dt className="text-muted-foreground">{t("invoices.due")}</dt>
          <dd>{inv.due_date?.slice(0, 10) ?? "—"}</dd>
          <dt className="text-muted-foreground">{t("invoices.total")}</dt>
          <dd className="tabular-nums">{String(inv.balance.total)}</dd>
          <dt className="text-muted-foreground">{t("invoices.paid")}</dt>
          <dd className="tabular-nums">{String(inv.balance.paid)}</dd>
          <dt className="text-muted-foreground">{t("invoices.open")}</dt>
          <dd className="tabular-nums">{String(inv.balance.open)}</dd>
        </dl>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="heading-2 text-base">{t("invoices.lines")}</h2>
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
                <TableCell>{item.title}</TableCell>
                <TableCell className="text-right tabular-nums">
                  {item.quantity}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {item.price_incl}
                </TableCell>
                <TableCell className="text-right tabular-nums">
                  {item.total_incl}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </section>

      <section className="flex flex-col gap-3">
        <h2 className="heading-2 text-base">{t("invoices.payments")}</h2>
        {payments.length === 0 ? (
          <Empty title={t("invoices.noPayments")} />
        ) : (
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>{t("invoices.date")}</TableHead>
                <TableHead>{t("invoices.method")}</TableHead>
                <TableHead className="text-right">
                  {t("invoices.amount")}
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {payments.map((pay) => (
                <TableRow key={pay.id}>
                  <TableCell>{pay.date.slice(0, 10)}</TableCell>
                  <TableCell>{pay.payment_method}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {pay.amount}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </section>
    </>
  )
}
