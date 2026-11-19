"use client"

import * as React from "react"
import Link from "next/link"
import { useLocale, useTranslations } from "next-intl"

import { useSession } from "@genesy/api-client/auth"
import { useSupplierBillsList } from "@genesy/api-client/endpoints/supplier-bills"
import { buttonVariants } from "@genesy/ui/components/button"
import { TableCell, TableRow } from "@genesy/ui/components/table"
import { formatDay } from "@genesy/ui/lib/format"
import { PageHead } from "@/components/list"
import { PayablesList, usePayWord } from "@/components/payables"
import { StatusBadge } from "@/components/status-badge"
import { useCan } from "@/lib/can"

import { showMoney } from "../purchases/shared"

/** Supplier bills (Template List, as /purchases): supplier filter, server totals, pager. */
export default function SupplierBillsPage({
  searchParams,
}: {
  searchParams: Promise<{ supplier?: string; page?: string }>
}) {
  const t = useTranslations()
  const locale = useLocale()
  const word = usePayWord()
  const mayCreate = useCan("supplierbill.create")
  const { supplier, page } = React.use(searchParams)
  const workspaceId = useSession((s) => s.workspaceId) ?? ""
  const bills = useSupplierBillsList(
    workspaceId,
    { supplier_id: supplier || undefined, page: page || undefined },
    { query: { enabled: Boolean(workspaceId) } }
  )
  return (
    <>
      <PageHead
        eyebrow={t("shell.buy")}
        title={t("payables.bills.title")}
        actions={
          mayCreate ? (
            <Link href="/supplier-bills/new" className={buttonVariants()}>
              {t("payables.bills.new")}
            </Link>
          ) : null
        }
      />
      <PayablesList
        supplier={supplier}
        query={bills}
        count={(total) => t("payables.bills.count", { total })}
        empty={t("payables.bills.empty")}
        columns={[
          { label: t("payables.number") },
          { label: t("payables.supplier") },
          { label: t("payables.state") },
          { label: t("payables.invoiceDate") },
          { label: t("payables.total"), right: true },
        ]}
        row={(b) => (
          <TableRow key={b.id}>
            <TableCell>
              <Link className="font-semibold underline-offset-4 hover:underline" href={`/supplier-bills/${b.id}`}>
                {t("payables.bills.detailTitle", { number: b.number })}
              </Link>
              <span className="block text-xs text-muted-foreground">{b.supplier_invoice_number}</span>
            </TableCell>
            <TableCell data-m="sub">{b.supplier.name ?? ""}</TableCell>
            <TableCell data-m="badge">
              <StatusBadge status={b.state} label={word("states", b.state)} />
            </TableCell>
            <TableCell data-m="hide" className="text-muted-foreground">
              {formatDay(b.invoice_date, locale)}
            </TableCell>
            <TableCell data-m="right" className="text-right font-semibold tabular-nums">
              {showMoney(b.totals.total ?? b.totals.net, b.currency, locale)}
            </TableCell>
          </TableRow>
        )}
      />
    </>
  )
}
